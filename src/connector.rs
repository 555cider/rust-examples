use crate::{config, entity};
use rustls::pki_types::{CertificateDer, ServerName};
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
    let root_store = tls_root_store(route)?;
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
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

fn tls_root_store(route: &config::Route) -> Result<rustls::RootCertStore, entity::GatewayError> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    if let Some(path) = route.tls.ca_cert_path.as_deref() {
        for cert in load_certificates(path)? {
            root_store.add(cert).map_err(|err| {
                entity::GatewayError::InvalidRouteConfig(format!(
                    "failed to add route tls certificate '{}': {}",
                    path, err
                ))
            })?;
        }
    }

    Ok(root_store)
}

fn load_certificates(path: &str) -> Result<Vec<CertificateDer<'static>>, entity::GatewayError> {
    let bytes = std::fs::read(path).map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "failed to read route tls certificate '{}': {}",
            path, err
        ))
    })?;

    if bytes.starts_with(b"-----BEGIN") {
        let mut reader = std::io::BufReader::new(bytes.as_slice());
        let certs = rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| {
                entity::GatewayError::InvalidRouteConfig(format!(
                    "failed to parse PEM certificate '{}': {}",
                    path, err
                ))
            })?;

        if certs.is_empty() {
            return Err(entity::GatewayError::InvalidRouteConfig(format!(
                "route tls certificate '{}' did not contain any certificates",
                path
            )));
        }

        Ok(certs)
    } else {
        Ok(vec![CertificateDer::from(bytes)])
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
