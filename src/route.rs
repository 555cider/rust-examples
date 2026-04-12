use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use crate::{config, entity, throttle, trace, upstream};
use hyper::{
    body::Incoming,
    header::{HeaderValue, HOST},
    service::service_fn,
    Request, Response,
};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder as AutoBuilder,
};
use tokio::io::{AsyncRead, AsyncWrite};

pub async fn run(
    gateway_stream: tokio::net::TcpStream,
    route_config_arr_clone: Arc<[config::Route]>,
    throttle_pool_clone: Arc<Mutex<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>>,
) -> Result<(), entity::GatewayError> {
    serve_connection(gateway_stream, route_config_arr_clone, throttle_pool_clone).await
}

pub async fn serve_connection<T>(
    gateway_stream: T,
    route_config_arr_clone: Arc<[config::Route]>,
    throttle_pool_clone: Arc<Mutex<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>>,
) -> Result<(), entity::GatewayError>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service = service_fn(move |incoming_request: Request<Incoming>| {
        let route_config_arr = route_config_arr_clone.clone();
        let throttle_pool = throttle_pool_clone.clone();

        async move {
            let throttle_status = throttle::run(
                &throttle_pool.lock().unwrap().get().unwrap(),
                incoming_request.headers(),
            )
            .unwrap();

            if !throttle_status.is_success() {
                return Ok::<Response<entity::GatewayBody>, entity::GatewayError>(
                    entity::get_gateway_response(throttle_status),
                );
            }

            let response = match route_request(incoming_request, &route_config_arr) {
                Ok(routed) => upstream::send(routed).await,
                Err(err) => Err(err),
            };

            match response {
                Ok(response) => Ok(response),
                Err(err) => match gateway_response_for_error(&err) {
                    Some(response) => Ok(response),
                    None => Err(err),
                },
            }
        }
    });

    AutoBuilder::new(TokioExecutor::new())
        .serve_connection(TokioIo::new(gateway_stream), service)
        .await
        .map_err(|err| entity::GatewayError::Io(std::io::Error::other(err)))
}

pub fn gateway_response_for_error(
    error: &entity::GatewayError,
) -> Option<Response<entity::GatewayBody>> {
    let status = match error {
        entity::GatewayError::RouteNotFound(_) => hyper::http::StatusCode::NOT_FOUND,
        entity::GatewayError::Timeout(_) => hyper::http::StatusCode::GATEWAY_TIMEOUT,
        entity::GatewayError::Hyper(_)
        | entity::GatewayError::Io(_)
        | entity::GatewayError::Http(_)
        | entity::GatewayError::InvalidRouteConfig(_) => hyper::http::StatusCode::BAD_GATEWAY,
    };

    Some(entity::get_gateway_response(status))
}

pub fn resolve_route(
    path: &str,
    route_config_arr: &[config::Route],
) -> Result<config::Route, entity::GatewayError> {
    config::get_route(path, route_config_arr)
        .cloned()
        .ok_or_else(|| entity::GatewayError::RouteNotFound(path.to_string()))
}

pub fn build_route_uri(uri: &hyper::Uri, route_config: &config::Route) -> String {
    build_route_uri_for_authority(uri, route_config, &route_config.authority)
}

pub fn build_route_uri_for_authority(
    uri: &hyper::Uri,
    route_config: &config::Route,
    authority: &config::Authority,
) -> String {
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");

    format!(
        "{}://{}{}",
        route_config.scheme,
        upstream_authority(authority),
        path_and_query
    )
}

pub fn route_request(
    incoming_request: Request<Incoming>,
    route_config_arr: &[config::Route],
) -> Result<upstream::RoutedRequest, entity::GatewayError> {
    let route_config = resolve_route(incoming_request.uri().path(), route_config_arr)?;
    let authority = select_upstream_authority(&route_config);
    let request = build_request(incoming_request, &route_config, &authority)?;

    Ok(upstream::RoutedRequest {
        route: route_config,
        authority,
        request,
    })
}

pub fn build_request(
    request: Request<Incoming>,
    route_config: &config::Route,
    authority: &config::Authority,
) -> Result<Request<Incoming>, entity::GatewayError> {
    let traceparent = extract_traceparent(request.headers());
    let uri = build_route_uri_for_authority(request.uri(), route_config, authority);

    let mut request_builder = Request::builder()
        .uri(uri)
        .method(request.method())
        .version(request.version());
    let headers = request_builder.headers_mut().unwrap();
    *headers = request.headers().clone();
    headers.insert(HOST, upstream_host_header(authority)?);
    headers.insert("traceparent", traceparent.as_headervalue());
    request_builder
        .body(request.into_body())
        .map_err(entity::GatewayError::from)
}

pub fn extract_traceparent(headers: &hyper::HeaderMap) -> trace::Traceparent {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| trace::extract(headers))) {
        Ok(Ok(traceparent)) => traceparent,
        Ok(Err(err)) => {
            log::warn!(
                "Failed to parse trace context. Generating new traceparent: {}",
                err
            );
            trace::new()
        }
        Err(_) => {
            log::warn!("Trace extraction panicked. Generating new traceparent");
            trace::new()
        }
    }
}

pub fn upstream_host_header(
    authority: &config::Authority,
) -> Result<HeaderValue, entity::GatewayError> {
    HeaderValue::from_str(upstream_authority(authority).as_str()).map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "invalid upstream authority '{}:{}': {}",
            authority.host, authority.port, err
        ))
    })
}

pub fn upstream_authority(authority: &config::Authority) -> String {
    match authority.host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(_)) => {
            format!("[{}]:{}", authority.host, authority.port)
        }
        _ => format!("{}:{}", authority.host, authority.port),
    }
}

fn select_upstream_authority(route_config: &config::Route) -> config::Authority {
    let pool_size = route_config.upstreams.len() + 1;
    let cursor = next_upstream_cursor(route_balance_key(route_config)) % pool_size;

    if cursor == 0 {
        route_config.authority.clone()
    } else {
        route_config.upstreams[cursor - 1].clone()
    }
}

fn route_balance_key(route_config: &config::Route) -> String {
    let mut key = format!(
        "{}|{:?}|{}|{}|{}",
        route_config.path,
        route_config.protocol,
        route_config.scheme,
        route_config.authority.host,
        route_config.authority.port
    );

    for authority in route_config.upstreams.iter() {
        key.push('|');
        key.push_str(&authority.host);
        key.push(':');
        key.push_str(&authority.port);
    }

    key
}

fn next_upstream_cursor(route_key: String) -> usize {
    static ROUTE_BALANCERS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

    let balancers = ROUTE_BALANCERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut balancers = balancers.lock().unwrap();
    let cursor = balancers.entry(route_key).or_insert(0);
    let selected = *cursor;
    *cursor = cursor.wrapping_add(1);
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Authority, Protocol, Route, RouteTlsConfig};
    use hyper::http::StatusCode;

    fn sample_route(protocol: Protocol) -> Route {
        Route {
            scheme: "http".to_string(),
            protocol,
            authority: Authority {
                host: "127.0.0.1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            path: "/".to_string(),
        }
    }

    #[test]
    fn build_route_uri_keeps_query_string() {
        let uri: hyper::Uri = "/product?id=7".parse().expect("uri");
        assert_eq!(
            build_route_uri(&uri, &sample_route(Protocol::Http1)),
            "http://127.0.0.1:9000/product?id=7"
        );
    }

    #[test]
    fn build_route_uri_brackets_ipv6_literals() {
        let uri: hyper::Uri = "/".parse().expect("uri");
        let route = Route {
            scheme: "http".to_string(),
            protocol: Protocol::Http1,
            authority: Authority {
                host: "::1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            path: "/".to_string(),
        };

        assert_eq!(build_route_uri(&uri, &route), "http://[::1]:9000/");
    }

    #[test]
    fn resolve_route_preserves_protocol() {
        let routes: Arc<[Route]> = Arc::from([sample_route(Protocol::H2c)]);

        let route = resolve_route("/", &routes).expect("route should exist");
        assert_eq!(route.protocol, Protocol::H2c);
    }

    #[test]
    fn route_not_found_maps_to_not_found_response() {
        let response = gateway_response_for_error(&entity::GatewayError::RouteNotFound(
            "/missing".to_string(),
        ))
        .expect("not found should map to response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn upstream_host_header_uses_selected_authority() {
        let header =
            upstream_host_header(&sample_route(Protocol::Http1).authority).expect("host header");

        assert_eq!(header, "127.0.0.1:9000");
    }

    #[test]
    fn upstream_host_header_brackets_ipv6_literals() {
        let route = Route {
            scheme: "http".to_string(),
            protocol: Protocol::Http1,
            authority: Authority {
                host: "::1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            path: "/".to_string(),
        };

        let header = upstream_host_header(&route.authority).expect("host header");

        assert_eq!(header, "[::1]:9000");
    }

    #[test]
    fn extract_traceparent_gracefully_falls_back_on_invalid_trace_context() {
        let mut headers = hyper::HeaderMap::new();
        headers.insert(
            "tracestate",
            hyper::header::HeaderValue::from_static("vendor=value"),
        );
        headers.insert(
            "traceparent",
            hyper::header::HeaderValue::from_static(
                "00-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz-0000000000000000-00",
            ),
        );

        let traceparent = extract_traceparent(&headers);

        assert_eq!(traceparent.as_string().len(), 55);
    }

    #[test]
    fn extract_traceparent_preserves_valid_traceparent_without_tracestate() {
        let mut headers = hyper::HeaderMap::new();
        headers.insert(
            "traceparent",
            hyper::header::HeaderValue::from_static(
                "00-00000000000000000000000000000001-0000000000000002-01",
            ),
        );

        let traceparent = extract_traceparent(&headers);

        assert_eq!(
            traceparent.as_string(),
            "00-00000000000000000000000000000001-0000000000000002-01"
        );
    }
}
