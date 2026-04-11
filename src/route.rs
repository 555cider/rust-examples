use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
};

use crate::{config, entity, throttle, trace};
use hyper::{body::Incoming, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;

pub async fn run(
    gateway_stream: tokio::net::TcpStream,
    route_config_arr_clone: Arc<[config::Route]>,
    throttle_pool_clone: Arc<Mutex<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>>,
) -> Result<(), entity::GatewayError> {
    let service_fn = service_fn(move |incoming_request: Request<Incoming>| {
            log::info!("incoming_request = {:?}", &incoming_request);

            let throttle_status: hyper::http::StatusCode = throttle::run(
                &throttle_pool_clone.lock().unwrap().get().unwrap(),
                incoming_request.headers(),
            )
            .unwrap();
            log::info!("throttle_status = {:?}", throttle_status);

            let (outgoing_request, addr) = route_request(incoming_request, &route_config_arr_clone);

            async move {
                if !throttle_status.is_success() {
                    return Ok::<Response<entity::GatewayBody>, entity::GatewayError>(
                        entity::get_gateway_response(throttle_status),
                    );
                }

                let route_stream = tokio::net::TcpStream::connect(addr).await?;
                let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
                    .handshake::<_, Incoming>(TokioIo::new(route_stream))
                    .await?;

                tokio::task::spawn(async move {
                    if let Err(err) = conn.await {
                        log::error!("Failed to spawn a connection: {:?}", err);
                    }
                });

                let incoming_response = sender.send_request(outgoing_request).await?;
                let (parts, body) = incoming_response.into_parts();
                let outgoing_response = Response::from_parts(parts, entity::GatewayBody::Incoming(body));
                log::info!("outgoing_response = {:?}", outgoing_response);
                Ok::<Response<entity::GatewayBody>, entity::GatewayError>(outgoing_response)
            }
        });

    match hyper::server::conn::http1::Builder::new()
        .serve_connection(TokioIo::new(gateway_stream), service_fn)
        .await
    {
        Ok(res) => Ok(res),
        Err(err) => {
            log::error!("Failed to bind a connection with a service: {:?}", err);
            Err(entity::GatewayError::from(err))
        }
    }
}

pub fn route_request(
    incoming_request: Request<Incoming>,
    route_config_arr: &[config::Route],
) -> (Request<Incoming>, SocketAddr) {
    log::info!("incoming_request = {:?}", &incoming_request);

    let route_config: config::Route =
        config::get_route(incoming_request.uri().path(), route_config_arr)
            .expect("Failed to get the routing configuration for the request!")
            .clone();

    let route_addr: SocketAddr = SocketAddr::from((
        route_config.authority.host.parse::<IpAddr>().unwrap(),
        route_config.authority.port.parse::<u16>().unwrap(),
    ));
    log::info!("Routed to {}://{:?}", route_config.scheme, &route_addr);

    (
        build_request(incoming_request, route_config).expect("Failed to create a routing request!"),
        route_addr,
    )
}

pub fn build_request(
    request: Request<Incoming>,
    route_config: config::Route,
) -> Result<Request<Incoming>, hyper::http::Error> {
    let uri: String = format!(
        "{}://{}:{}{}",
        route_config.scheme,
        route_config.authority.host,
        route_config.authority.port,
        request.uri().path()
    );

    let traceparent: trace::Traceparent =
        trace::extract(request.headers()).expect("Failed to extract trace context!");

    let mut request_builder = Request::builder()
        .uri(uri)
        .method(request.method())
        .version(request.version());
    *request_builder.headers_mut().unwrap() = request.headers().clone();
    request_builder
        .headers_mut()
        .unwrap()
        .append("traceparent", traceparent.as_headervalue());
    request_builder.body(request.into_body())
}
