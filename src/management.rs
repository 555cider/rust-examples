use crate::{config, entity};
use hyper::{body::Bytes, Method, Request, Response, StatusCode};
use serde::Serialize;

const HEALTH_PATH: &str = "/__gateway/health";
const ROUTES_PATH: &str = "/__gateway/routes";

pub fn handle(
    request: &Request<hyper::body::Incoming>,
    routes: &[config::Route],
) -> Result<Option<Response<entity::GatewayBody>>, entity::GatewayError> {
    match request.uri().path() {
        HEALTH_PATH => Ok(Some(handle_health(request.method()))),
        ROUTES_PATH => Ok(Some(handle_routes(request.method(), routes)?)),
        _ => Ok(None),
    }
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

#[derive(Serialize)]
struct RoutesPayload<'a> {
    routes: &'a [config::Route],
}
