use crate::{config, entity};
use hyper::{body::Incoming, Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::net::{IpAddr, SocketAddr};

pub struct RoutedRequest {
    pub addr: SocketAddr,
    pub protocol: config::Protocol,
    pub request: Request<Incoming>,
}

pub fn route_addr(route: &config::Route) -> Result<SocketAddr, entity::GatewayError> {
    let host = route.authority.host.parse::<IpAddr>().map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "invalid route host '{}': {}",
            route.authority.host, err
        ))
    })?;
    let port = route.authority.port.parse::<u16>().map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "invalid route port '{}': {}",
            route.authority.port, err
        ))
    })?;

    Ok(SocketAddr::from((host, port)))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_route(host: &str, port: &str) -> config::Route {
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: host.to_string(),
                port: port.to_string(),
            },
            path: "/".to_string(),
        }
    }

    #[test]
    fn route_addr_rejects_invalid_host() {
        let err = route_addr(&sample_route("not-an-ip", "9000")).expect_err("invalid host");

        assert!(matches!(err, entity::GatewayError::InvalidRouteConfig(_)));
    }

    #[test]
    fn route_addr_rejects_invalid_port() {
        let err = route_addr(&sample_route("127.0.0.1", "abc")).expect_err("invalid port");

        assert!(matches!(err, entity::GatewayError::InvalidRouteConfig(_)));
    }
}
