use crate::{config, connector, entity, route};
use http_body_util::Full;
use hyper::{body::Bytes, header::HOST, Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
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

pub async fn send(
    routed: RoutedRequest,
) -> Result<Response<entity::GatewayBody>, entity::GatewayError> {
    let max_attempts = routed.route.retry_attempts + 1;
    let mut last_error = None;

    for attempt in 0..max_attempts {
        let authority = route::authority_for_attempt(&routed.route, routed.start_index, attempt);
        let request = build_attempt_request(&routed, authority)?;

        match send_attempt(&routed.route, authority, request).await {
            Ok(response) => return Ok(response),
            Err(err) if should_retry(&err) && attempt + 1 < max_attempts => {
                log::warn!(
                    "Upstream attempt {}/{} failed for route '{}': {}",
                    attempt + 1,
                    max_attempts,
                    routed.route.path,
                    err
                );
                last_error = Some(err);
            }
            Err(err) => return Err(err),
        }
    }

    Err(last_error.expect("retry loop should preserve the last retryable error"))
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
) -> Result<Response<entity::GatewayBody>, entity::GatewayError> {
    let protocol = route.protocol.clone();
    let timeout_ms = route.timeout_ms;
    let stream = connector::connect(route, authority).await?;

    let response = async move {
        match stream {
            connector::UpstreamStream::Plain(stream) => {
                send_on_io(TokioIo::new(stream), protocol, request).await
            }
            connector::UpstreamStream::Tls(stream) => {
                send_on_io(TokioIo::new(stream), protocol, request).await
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

async fn send_on_io<T>(
    route_io: TokioIo<T>,
    protocol: config::Protocol,
    request: Request<Full<Bytes>>,
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
            let (parts, body) = response.into_parts();
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
            let (parts, body) = response.into_parts();
            Ok(Response::from_parts(
                parts,
                entity::GatewayBody::Incoming(body),
            ))
        }
    }
}
