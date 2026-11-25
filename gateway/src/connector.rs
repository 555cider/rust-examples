use crate::{config, discovery, entity, tls};
use rustls::pki_types::ServerName;
use std::{
    collections::HashMap,
    convert::TryFrom,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Debug)]
pub enum UpstreamStream {
    Plain(tokio::net::TcpStream),
    Tls(tokio_rustls::client::TlsStream<tokio::net::TcpStream>),
}

pub async fn connect(
    route: &config::Route,
    authority: &config::Authority,
    discovery_registry: &discovery::DiscoveryRegistry,
) -> Result<UpstreamStream, entity::GatewayError> {
    let resolved_authority =
        if let Some(discovered) = discovery::next_authority(discovery_registry, route).await? {
            discovered
        } else {
            authority.clone()
        };
    let stream = connect_resolved(&resolved_authority).await?;

    if matches!(route.protocol, config::Protocol::H2) && route.scheme != "https" {
        return Err(entity::GatewayError::InvalidRouteConfig(
            "protocol 'h2' requires scheme 'https'".to_string(),
        ));
    }

    match route.scheme.as_str() {
        "http" => Ok(UpstreamStream::Plain(stream)),
        "https" => Ok(UpstreamStream::Tls(
            connect_tls(route, &resolved_authority, stream).await?,
        )),
        scheme => Err(entity::GatewayError::InvalidRouteConfig(format!(
            "unsupported route scheme '{}'",
            scheme
        ))),
    }
}

/// Connects to the round-robin pick first, then falls back to the other resolved
/// addresses (e.g. `localhost` resolving to `::1` while the upstream listens on IPv4 only).
async fn connect_resolved(
    authority: &config::Authority,
) -> Result<tokio::net::TcpStream, entity::GatewayError> {
    let resolved = resolve_socket_addrs(authority).await?;
    let mut last_error = None;
    for addr in connection_order(authority, &resolved) {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(stream) => return Ok(stream),
            Err(err) => last_error = Some(err),
        }
    }
    Err(match last_error {
        Some(err) => err.into(),
        None => entity::GatewayError::InvalidRouteConfig(format!(
            "no socket addresses resolved for authority '{}:{}'",
            authority.host, authority.port
        )),
    })
}

async fn resolve_socket_addrs(
    authority: &config::Authority,
) -> Result<Vec<SocketAddr>, entity::GatewayError> {
    let port = authority.port.parse::<u16>().map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "invalid route port '{}': {}",
            authority.port, err
        ))
    })?;

    if let Ok(ip_addr) = authority.host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip_addr, port)]);
    }

    let resolved = tokio::net::lookup_host((authority.host.as_str(), port))
        .await?
        .collect::<Vec<_>>();
    if resolved.is_empty() {
        return Err(entity::GatewayError::InvalidRouteConfig(format!(
            "no addresses resolved for authority '{}:{}'",
            authority.host, authority.port
        )));
    }
    Ok(resolved)
}

fn connection_order(authority: &config::Authority, resolved: &[SocketAddr]) -> Vec<SocketAddr> {
    if resolved.is_empty() {
        return Vec::new();
    }

    let start = next_resolved_cursor(authority_balance_key(authority)) % resolved.len();
    resolved[start..]
        .iter()
        .chain(&resolved[..start])
        .copied()
        .collect()
}

#[cfg(test)]
fn select_resolved_address(
    authority: &config::Authority,
    resolved: &[SocketAddr],
) -> Option<SocketAddr> {
    connection_order(authority, resolved).first().copied()
}

fn authority_balance_key(authority: &config::Authority) -> String {
    format!("{}:{}", authority.host, authority.port)
}

fn next_resolved_cursor(authority_key: String) -> usize {
    static RESOLVERS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

    let resolvers = RESOLVERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut resolvers = resolvers.lock().unwrap();
    let cursor = resolvers.entry(authority_key).or_insert(0);
    let selected = *cursor;
    *cursor = cursor.wrapping_add(1);
    selected
}

async fn connect_tls(
    route: &config::Route,
    authority: &config::Authority,
    stream: tokio::net::TcpStream,
) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>, entity::GatewayError> {
    let server_name = tls_server_name(&authority.host)?;
    let client_config =
        tls::build_client_config(&route.protocol, route.tls.ca_cert_path.as_deref())?;
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));

    connector
        .connect(server_name, stream)
        .await
        .map_err(Into::into)
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
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    fn sample_route(host: &str, port: &str) -> config::Route {
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: host.to_string(),
                port: port.to_string(),
            },
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: std::collections::BTreeMap::new(),
            queries: std::collections::BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
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

        let route = sample_route("localhost", &port.to_string());
        let stream = connect(&route, &route.authority, &None)
            .await
            .expect("hostname should resolve");

        assert!(matches!(stream, UpstreamStream::Plain(_)));
        accept_task.await.expect("accept task");
    }

    #[test]
    fn rejects_invalid_port() {
        let route = sample_route("localhost", "abc");
        let err = connect(&route, &route.authority, &None);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        let err = runtime
            .block_on(err)
            .expect_err("invalid port should fail before connect");

        assert!(matches!(err, entity::GatewayError::InvalidRouteConfig(_)));
    }

    #[test]
    fn selects_resolved_addresses_round_robin() {
        let authority = config::Authority {
            host: format!("round-robin-{}", fastrand::u64(..)),
            port: "8080".to_string(),
        };
        let addresses = vec![
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 2), 8080)),
        ];

        let first = select_resolved_address(&authority, &addresses).expect("first address");
        let second = select_resolved_address(&authority, &addresses).expect("second address");
        let third = select_resolved_address(&authority, &addresses).expect("third address");

        assert_eq!(first, addresses[0]);
        assert_eq!(second, addresses[1]);
        assert_eq!(third, addresses[0]);
    }
}
