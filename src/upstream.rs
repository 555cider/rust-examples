use crate::{config, connector, discovery, entity, route};
use http_body_util::Full;
use hyper::{body::Bytes, header::HOST, Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct RoutedRequest {
    pub route: config::Route,
    pub start_index: usize,
    pub method: hyper::Method,
    pub version: hyper::Version,
    pub headers: hyper::HeaderMap,
    pub path_and_query: String,
    pub body: Bytes,
}

#[derive(Default)]
struct CircuitBreakerState {
    consecutive_failures: usize,
    open_until: Option<Instant>,
}

pub async fn send(
    routed: RoutedRequest,
    discovery_registry: discovery::DiscoveryRegistry,
) -> Result<Response<entity::GatewayBody>, entity::GatewayError> {
    let circuit_breaker = validate_circuit_breaker_config(&routed.route)?;
    enforce_circuit_breaker(&routed.route, circuit_breaker)?;

    let max_attempts = routed.route.retry_attempts + 1;
    let mut last_error = None;

    for attempt in 0..max_attempts {
        let authority = route::authority_for_attempt(&routed.route, routed.start_index, attempt);
        let request = build_attempt_request(&routed, authority)?;

        match send_attempt(&routed.route, authority, request, &discovery_registry).await {
            Ok(response)
                if should_retry_status(response.status(), &routed.route)
                    && attempt + 1 < max_attempts =>
            {
                log::warn!(
                    "Upstream attempt {}/{} returned retryable status {} for route '{}'",
                    attempt + 1,
                    max_attempts,
                    response.status(),
                    routed.route.path
                );
                wait_before_retry(&routed.route).await;
            }
            Ok(response) => {
                reset_circuit_breaker(&routed.route);
                return Ok(response);
            }
            Err(err) if should_retry(&err) && attempt + 1 < max_attempts => {
                log::warn!(
                    "Upstream attempt {}/{} failed for route '{}': {}",
                    attempt + 1,
                    max_attempts,
                    routed.route.path,
                    err
                );
                last_error = Some(err);
                wait_before_retry(&routed.route).await;
            }
            Err(err) => {
                record_circuit_breaker_failure(&routed.route, circuit_breaker, &err);
                return Err(err);
            }
        }
    }

    let err = last_error.expect("retry loop should preserve the last retryable error");
    record_circuit_breaker_failure(&routed.route, circuit_breaker, &err);
    Err(err)
}

fn build_attempt_request(
    routed: &RoutedRequest,
    authority: &config::Authority,
) -> Result<Request<Full<Bytes>>, entity::GatewayError> {
    let uri = format!(
        "{}://{}{}",
        routed.route.scheme,
        route::upstream_authority(authority),
        routed.path_and_query
    );
    let mut request_builder = Request::builder()
        .uri(uri)
        .method(routed.method.clone())
        .version(routed.version);
    let headers = request_builder.headers_mut().unwrap();
    *headers = routed.headers.clone();
    headers.insert(HOST, route::upstream_host_header(authority)?);
    request_builder
        .body(Full::new(routed.body.clone()))
        .map_err(entity::GatewayError::from)
}

async fn send_attempt(
    route: &config::Route,
    authority: &config::Authority,
    request: Request<Full<Bytes>>,
    discovery_registry: &discovery::DiscoveryRegistry,
) -> Result<Response<entity::GatewayBody>, entity::GatewayError> {
    let protocol = route.protocol.clone();
    let timeout_ms = route.timeout_ms;
    let response_header_filter = route.response_header_filter.clone();
    let stream = connector::connect(route, authority, discovery_registry).await?;

    let response = async move {
        match stream {
            connector::UpstreamStream::Plain(stream) => {
                send_on_io(
                    TokioIo::new(stream),
                    protocol,
                    request,
                    &response_header_filter,
                )
                .await
            }
            connector::UpstreamStream::Tls(stream) => {
                send_on_io(
                    TokioIo::new(stream),
                    protocol,
                    request,
                    &response_header_filter,
                )
                .await
            }
        }
    };

    tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), response)
        .await
        .map_err(entity::GatewayError::from)?
}

fn should_retry(error: &entity::GatewayError) -> bool {
    matches!(
        error,
        entity::GatewayError::Hyper(_)
            | entity::GatewayError::Io(_)
            | entity::GatewayError::Timeout(_)
    )
}

fn should_retry_status(status: hyper::StatusCode, route: &config::Route) -> bool {
    route
        .retry_statuses
        .iter()
        .any(|code| *code == status.as_u16())
}

async fn wait_before_retry(route: &config::Route) {
    if route.retry_backoff_ms > 0 {
        tokio::time::sleep(Duration::from_millis(route.retry_backoff_ms)).await;
    }
}

fn should_trip_circuit_breaker(error: &entity::GatewayError) -> bool {
    should_retry(error)
}

fn validate_circuit_breaker_config(
    route: &config::Route,
) -> Result<Option<&config::CircuitBreakerConfig>, entity::GatewayError> {
    let Some(circuit_breaker) = route.circuit_breaker.as_ref() else {
        return Ok(None);
    };

    if circuit_breaker.failure_threshold == 0 {
        return Err(entity::GatewayError::InvalidRouteConfig(format!(
            "route '{}' circuit_breaker.failure_threshold must be greater than 0",
            route.path
        )));
    }

    if circuit_breaker.open_ms == 0 {
        return Err(entity::GatewayError::InvalidRouteConfig(format!(
            "route '{}' circuit_breaker.open_ms must be greater than 0",
            route.path
        )));
    }

    Ok(Some(circuit_breaker))
}

fn enforce_circuit_breaker(
    route: &config::Route,
    circuit_breaker: Option<&config::CircuitBreakerConfig>,
) -> Result<(), entity::GatewayError> {
    let Some(_) = circuit_breaker else {
        return Ok(());
    };

    let now = Instant::now();
    let mut states = circuit_breaker_states().lock().unwrap();
    let Some(state) = states.get_mut(&route.path) else {
        return Ok(());
    };

    match state.open_until {
        Some(open_until) if open_until > now => {
            Err(entity::GatewayError::CircuitOpen(route.path.clone()))
        }
        Some(_) => {
            state.open_until = None;
            Ok(())
        }
        None => Ok(()),
    }
}

fn reset_circuit_breaker(route: &config::Route) {
    if route.circuit_breaker.is_none() {
        return;
    }

    circuit_breaker_states().lock().unwrap().remove(&route.path);
}

fn record_circuit_breaker_failure(
    route: &config::Route,
    circuit_breaker: Option<&config::CircuitBreakerConfig>,
    error: &entity::GatewayError,
) {
    let Some(circuit_breaker) = circuit_breaker else {
        return;
    };

    if !should_trip_circuit_breaker(error) {
        return;
    }

    let mut states = circuit_breaker_states().lock().unwrap();
    let state = states.entry(route.path.clone()).or_default();
    state.consecutive_failures += 1;

    if state.consecutive_failures >= circuit_breaker.failure_threshold {
        state.open_until = Some(Instant::now() + Duration::from_millis(circuit_breaker.open_ms));
    }
}

fn circuit_breaker_states() -> &'static Mutex<HashMap<String, CircuitBreakerState>> {
    static STATES: OnceLock<Mutex<HashMap<String, CircuitBreakerState>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn send_on_io<T>(
    route_io: TokioIo<T>,
    protocol: config::Protocol,
    request: Request<Full<Bytes>>,
    response_header_filter: &config::HeaderFilterConfig,
) -> Result<Response<entity::GatewayBody>, entity::GatewayError>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    match protocol {
        config::Protocol::Http1 => {
            let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
                .handshake::<_, Full<Bytes>>(route_io)
                .await?;

            tokio::spawn(async move {
                if let Err(err) = conn.await {
                    log::error!("HTTP/1 upstream connection failed: {:?}", err);
                }
            });

            let response = sender.send_request(request).await?;
            let (mut parts, body) = response.into_parts();
            route::apply_header_filter(&mut parts.headers, response_header_filter)?;
            Ok(Response::from_parts(
                parts,
                entity::GatewayBody::Incoming(body),
            ))
        }
        config::Protocol::H2c | config::Protocol::H2 => {
            let (mut sender, conn) = hyper::client::conn::http2::Builder::new(TokioExecutor::new())
                .handshake::<_, Full<Bytes>>(route_io)
                .await?;

            tokio::spawn(async move {
                if let Err(err) = conn.await {
                    log::error!("HTTP/2 upstream connection failed: {:?}", err);
                }
            });

            let response = sender.send_request(request).await?;
            let (mut parts, body) = response.into_parts();
            route::apply_header_filter(&mut parts.headers, response_header_filter)?;
            Ok(Response::from_parts(
                parts,
                entity::GatewayBody::Incoming(body),
            ))
        }
    }
}
