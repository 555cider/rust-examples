use crate::{config, entity, tls};
use rustls::pki_types::ServerName;
use std::{convert::TryFrom, net::IpAddr, sync::Arc};

#[derive(Debug)]
pub enum UpstreamStream {
    Plain(tokio::net::TcpStream),
    Tls(tokio_rustls::client::TlsStream<tokio::net::TcpStream>),
}

pub async fn connect(route: &config::Route) -> Result<UpstreamStream, entity::GatewayError> {
    let port = route.authority.port.parse::<u16>().map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "invalid route port '{}': {}",
            route.authority.port, err
        ))
    })?;

    let stream = tokio::net::TcpStream::connect((route.authority.host.as_str(), port)).await?;

    if matches!(route.protocol, config::Protocol::H2) && route.scheme != "https" {
        return Err(entity::GatewayError::InvalidRouteConfig(
            "protocol 'h2' requires scheme 'https'".to_string(),
        ));
    }

    match route.scheme.as_str() {
        "http" => Ok(UpstreamStream::Plain(stream)),
        "https" => Ok(UpstreamStream::Tls(connect_tls(route, stream).await?)),
        scheme => Err(entity::GatewayError::InvalidRouteConfig(format!(
            "unsupported route scheme '{}'",
            scheme
        ))),
    }
}

async fn connect_tls(
    route: &config::Route,
    stream: tokio::net::TcpStream,
) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>, entity::GatewayError> {
    let server_name = tls_server_name(&route.authority.host)?;
    let client_config = tls::build_client_config(&route.protocol, route.tls.ca_cert_path.as_deref())?;
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));

    connector.connect(server_name, stream).await.map_err(Into::into)
}

fn tls_server_name(host: &str) -> Result<ServerName<'static>, entity::GatewayError> {
    if let Ok(ip_addr) = host.parse::<IpAddr>() {
        Ok(ServerName::IpAddress(ip_addr.into()))
    } else {
        ServerName::try_from(host.to_string()).map_err(|err| {
            entity::GatewayError::InvalidRouteConfig(format!(
                "invalid tls server name '{}': {}",
                host, err
            ))
        })
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
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            path: "/".to_string(),
        }
    }

    #[tokio::test]
    async fn accepts_hostname_targets() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("listener");
        let port = listener.local_addr().expect("local addr").port();

        let accept_task = tokio::spawn(async move {
            let _ = listener.accept().await.expect("accepted connection");
        });

        let stream = connect(&sample_route("localhost", &port.to_string()))
            .await
            .expect("hostname should resolve");

        assert!(matches!(stream, UpstreamStream::Plain(_)));
        accept_task.await.expect("accept task");
    }

    #[test]
    fn rejects_invalid_port() {
        let route = sample_route("localhost", "abc");
        let err = connect(&route);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        let err = runtime
            .block_on(err)
            .expect_err("invalid port should fail before connect");

        assert!(matches!(err, entity::GatewayError::InvalidRouteConfig(_)));
    }
}
