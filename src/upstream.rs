use crate::{config, entity};
use hyper::{body::Incoming, Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::net::{IpAddr, SocketAddr};

pub struct RoutedRequest {
    pub addr: SocketAddr,
    pub protocol: config::Protocol,
    pub request: Request<Incoming>,
}

pub fn route_addr(route: &config::Route) -> SocketAddr {
    SocketAddr::from((
        route.authority.host.parse::<IpAddr>().unwrap(),
        route.authority.port.parse::<u16>().unwrap(),
    ))
}

pub async fn send(
    routed: RoutedRequest,
) -> Result<Response<entity::GatewayBody>, entity::GatewayError> {
    let route_stream = tokio::net::TcpStream::connect(routed.addr).await?;
    let route_io = TokioIo::new(route_stream);

    match routed.protocol {
        config::Protocol::Http1 => {
            let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
                .handshake::<_, Incoming>(route_io)
                .await?;

            tokio::spawn(async move {
                if let Err(err) = conn.await {
                    log::error!("HTTP/1 upstream connection failed: {:?}", err);
                }
            });

            let response = sender.send_request(routed.request).await?;
            let (parts, body) = response.into_parts();
            Ok(Response::from_parts(
                parts,
                entity::GatewayBody::Incoming(body),
            ))
        }
        config::Protocol::H2c => {
            let (mut sender, conn) = hyper::client::conn::http2::Builder::new(TokioExecutor::new())
                .handshake::<_, Incoming>(route_io)
                .await?;

            tokio::spawn(async move {
                if let Err(err) = conn.await {
                    log::error!("HTTP/2 upstream connection failed: {:?}", err);
                }
            });

            let response = sender.send_request(routed.request).await?;
            let (parts, body) = response.into_parts();
            Ok(Response::from_parts(
                parts,
                entity::GatewayBody::Incoming(body),
            ))
        }
    }
}
