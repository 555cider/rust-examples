use crate::{config, entity, metrics};
use hyper::{body::Bytes, Method, Request, Response, StatusCode};
use serde::Serialize;

const HEALTH_PATH: &str = "/__gateway/health";
const ROUTES_PATH: &str = "/__gateway/routes";
const METRICS_PATH: &str = "/__gateway/metrics";

pub fn handle(
    request: &Request<hyper::body::Incoming>,
    routes: &[config::Route],
    metrics_collector: &metrics::MetricsCollector,
) -> Result<Option<Response<entity::GatewayBody>>, entity::GatewayError> {
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

#[derive(Serialize)]
struct RoutesPayload<'a> {
    routes: &'a [config::Route],
}
