use crate::{config, entity, metrics};
use hyper::{body::Bytes, Method, Request, Response, StatusCode};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder as AutoBuilder,
};
use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncWrite};

const HEALTH_PATH: &str = "/__gateway/health";
const ROUTES_PATH: &str = "/__gateway/routes";
const METRICS_PATH: &str = "/__gateway/metrics";

pub async fn run<T>(
    gateway_stream: T,
    routes: Arc<[config::Route]>,
    metrics_collector: metrics::MetricsCollector,
    management_config: config::ManagementConfig,
) -> Result<(), entity::GatewayError>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service =
        hyper::service::service_fn(move |incoming_request: Request<hyper::body::Incoming>| {
            let routes = routes.clone();
            let metrics_collector = metrics_collector.clone();
            let management_config = management_config.clone();

            async move {
                let request_path = incoming_request.uri().path().to_string();
                let request_started_at = Instant::now();

                if let Some(response) = handle(
                    &incoming_request,
                    &routes,
                    &metrics_collector,
                    &management_config,
                )? {
                    if !is_metrics_path(&request_path) {
                        metrics::record(
                            &metrics_collector,
                            &request_path,
                            response.status(),
                            request_started_at.elapsed(),
                            None,
                        );
                    }
                    return Ok::<Response<entity::GatewayBody>, entity::GatewayError>(response);
                }

                let response = entity::get_gateway_response(StatusCode::NOT_FOUND);
                metrics::record(
                    &metrics_collector,
                    "__unmatched",
                    response.status(),
                    request_started_at.elapsed(),
                    None,
                );
                Ok(response)
            }
        });

    AutoBuilder::new(TokioExecutor::new())
        .serve_connection(TokioIo::new(gateway_stream), service)
        .await
        .map_err(|err| entity::GatewayError::Io(std::io::Error::other(err)))
}

pub fn handle(
    request: &Request<hyper::body::Incoming>,
    routes: &[config::Route],
    metrics_collector: &metrics::MetricsCollector,
    management_config: &config::ManagementConfig,
) -> Result<Option<Response<entity::GatewayBody>>, entity::GatewayError> {
    match request.uri().path() {
        HEALTH_PATH | ROUTES_PATH | METRICS_PATH => {
            if let Some(response) = unauthorized_management_response(request, management_config) {
                return Ok(Some(response));
            }
        }
        _ => {}
    }

    match request.uri().path() {
        HEALTH_PATH => Ok(Some(handle_health(request.method()))),
        ROUTES_PATH => Ok(Some(handle_routes(request.method(), routes)?)),
        METRICS_PATH => Ok(Some(handle_metrics(request.method(), metrics_collector)?)),
        _ => Ok(None),
    }
}

pub fn is_metrics_path(path: &str) -> bool {
    path == METRICS_PATH
}

pub fn is_management_path(path: &str) -> bool {
    matches!(path, HEALTH_PATH | ROUTES_PATH | METRICS_PATH)
}

fn handle_health(method: &Method) -> Response<entity::GatewayBody> {
    if *method != Method::GET {
        return entity::get_gateway_response(StatusCode::METHOD_NOT_ALLOWED);
    }

    entity::get_gateway_json_response(StatusCode::OK, Bytes::from_static(br#"{"status":"UP"}"#))
}

fn handle_routes(
    method: &Method,
    routes: &[config::Route],
) -> Result<Response<entity::GatewayBody>, entity::GatewayError> {
    if *method != Method::GET {
        return Ok(entity::get_gateway_response(StatusCode::METHOD_NOT_ALLOWED));
    }

    let payload = RoutesPayload { routes };
    let body = serde_json::to_vec(&payload).map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "failed to serialize management routes: {}",
            err
        ))
    })?;

    Ok(entity::get_gateway_json_response(
        StatusCode::OK,
        Bytes::from(body),
    ))
}

fn handle_metrics(
    method: &Method,
    metrics_collector: &metrics::MetricsCollector,
) -> Result<Response<entity::GatewayBody>, entity::GatewayError> {
    if *method != Method::GET {
        return Ok(entity::get_gateway_response(StatusCode::METHOD_NOT_ALLOWED));
    }

    let snapshot = metrics::snapshot(metrics_collector);
    let body = serde_json::to_vec(&snapshot).map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "failed to serialize management metrics: {}",
            err
        ))
    })?;

    Ok(entity::get_gateway_json_response(
        StatusCode::OK,
        Bytes::from(body),
    ))
}

fn unauthorized_management_response(
    request: &Request<hyper::body::Incoming>,
    management_config: &config::ManagementConfig,
) -> Option<Response<entity::GatewayBody>> {
    let Some(auth_token) = management_config.auth_token.as_deref() else {
        return None;
    };

    let authorization = request
        .headers()
        .get(hyper::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let expected = format!("Bearer {}", auth_token);
    if authorization == Some(expected.as_str()) {
        None
    } else {
        Some(
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(hyper::header::WWW_AUTHENTICATE, "Bearer")
                .body(entity::GatewayBody::Empty)
                .expect("unauthorized management response"),
        )
    }
}

#[derive(Serialize)]
struct RoutesPayload<'a> {
    routes: &'a [config::Route],
}
