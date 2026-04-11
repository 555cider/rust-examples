use crate::{config, entity};

#[derive(Debug)]
pub enum UpstreamStream {
    Plain(tokio::net::TcpStream),
}

pub async fn connect(route: &config::Route) -> Result<UpstreamStream, entity::GatewayError> {
    let port = route.authority.port.parse::<u16>().map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "invalid route port '{}': {}",
            route.authority.port, err
        ))
    })?;

    let stream = tokio::net::TcpStream::connect((route.authority.host.as_str(), port)).await?;
    Ok(UpstreamStream::Plain(stream))
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
