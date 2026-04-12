use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::File, io::Read, sync::Arc};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GatewayConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(alias = "route")]
    pub routes: Arc<[Route]>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen: String,
    #[serde(default)]
    pub tls: Option<ServerTlsConfig>,
    #[serde(default)]
    pub management: ManagementConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen_addr(),
            tls: None,
            management: ManagementConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Http1,
    H2c,
    H2,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Route {
    pub scheme: String,
    pub protocol: Protocol,
    pub authority: Authority,
    #[serde(default)]
    pub upstreams: Arc<[Authority]>,
    #[serde(default)]
    pub hosts: Arc<[String]>,
    #[serde(default)]
    pub methods: Arc<[String]>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub queries: BTreeMap<String, String>,
    #[serde(default)]
    pub tls: RouteTlsConfig,
    #[serde(default = "default_route_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_route_retry_attempts")]
    pub retry_attempts: usize,
    #[serde(default)]
    pub retry_statuses: Arc<[u16]>,
    #[serde(default = "default_route_retry_backoff_ms")]
    pub retry_backoff_ms: u64,
    #[serde(default)]
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    #[serde(default)]
    pub rate_limit: Option<RateLimitConfig>,
    #[serde(default)]
    pub rewrite_prefix: Option<String>,
    #[serde(default)]
    pub request_header_filter: HeaderFilterConfig,
    #[serde(default)]
    pub response_header_filter: HeaderFilterConfig,
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Authority {
    pub host: String,
    pub port: String,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct HeaderFilterConfig {
    #[serde(default)]
    pub set: BTreeMap<String, String>,
    #[serde(default)]
    pub remove: Arc<[String]>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: usize,
    pub open_ms: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct RateLimitConfig {
    pub requests: u64,
    pub window_ms: u64,
    #[serde(default = "default_rate_limit_key_header")]
    pub key_header: String,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct RouteTlsConfig {
    #[serde(default)]
    pub ca_cert_path: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ServerTlsConfig {
    #[serde(default)]
    pub cert_path: Option<String>,
    #[serde(default)]
    pub key_path: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ManagementConfig {
    #[serde(default)]
    pub auth_token: Option<String>,
}

fn default_listen_addr() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_route_timeout_ms() -> u64 {
    30_000
}

fn default_route_retry_attempts() -> usize {
    0
}

fn default_route_retry_backoff_ms() -> u64 {
    0
}

fn default_rate_limit_key_header() -> String {
    "api-key".to_string()
}

pub fn load_config(path: &str) -> GatewayConfig {
    let mut contents: String = String::new();
    let mut file: File = File::open(path).expect("Failed to open the configuration file!");
    file.read_to_string(&mut contents)
        .expect("Failed to read the configuration file!");
    serde_yaml::from_str(&contents).expect("Failed to parse the configuration file!")
}

pub fn get_route<'a>(path: &str, route: &'a [Route]) -> Option<&'a Route> {
    route
        .iter()
        .filter(|candidate| path.starts_with(&candidate.path))
        .max_by_key(|candidate| candidate.path.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_listen_and_routes_alias() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
server:
  listen: 127.0.0.1:9090
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    path: /
"#,
        )
        .expect("config should parse");

        assert_eq!(config.server.listen, "127.0.0.1:9090");
        assert_eq!(config.routes[0].authority.host, "localhost");
    }

    #[test]
    fn parses_server_tls_config() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
server:
  listen: 127.0.0.1:9443
  tls:
    cert_path: /tmp/gateway-cert.der
    key_path: /tmp/gateway-key.der
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    path: /
"#,
        )
        .expect("tls config should parse");

        let tls = config.server.tls.expect("server tls");
        assert_eq!(tls.cert_path.as_deref(), Some("/tmp/gateway-cert.der"));
        assert_eq!(tls.key_path.as_deref(), Some("/tmp/gateway-key.der"));
    }

    #[test]
    fn parses_server_management_auth_token() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
server:
  management:
    auth_token: secret-token
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    path: /
"#,
        )
        .expect("management auth config should parse");

        assert_eq!(
            config.server.management.auth_token.as_deref(),
            Some("secret-token")
        );
    }

    #[test]
    fn keeps_legacy_route_key_compatible() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
route:
  - scheme: http
    protocol: http1
    authority:
      host: 127.0.0.1
      port: 8001
    path: /
"#,
        )
        .expect("legacy config should parse");

        assert_eq!(config.server.listen, "127.0.0.1:8080");
        assert_eq!(config.routes.len(), 1);
    }

    #[test]
    fn parses_http1_and_h2c_routes() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
route:
  - scheme: http
    protocol: http1
    authority:
      host: 127.0.0.1
      port: 8001
    path: /http1
  - scheme: http
    protocol: h2c
    authority:
      host: 127.0.0.1
      port: 8002
    path: /h2
"#,
        )
        .expect("config should parse");

        assert_eq!(config.routes[0].protocol, Protocol::Http1);
        assert_eq!(config.routes[1].protocol, Protocol::H2c);
    }

    #[test]
    fn parses_h2_routes() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: https
    protocol: h2
    authority:
      host: localhost
      port: 8443
    path: /
"#,
        )
        .expect("h2 config should parse");

        assert_eq!(config.routes[0].protocol, Protocol::H2);
    }

    #[test]
    fn parses_route_timeout_and_defaults() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    timeout_ms: 250
    path: /fast
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8002
    path: /
"#,
        )
        .expect("timeout config should parse");

        assert_eq!(config.routes[0].timeout_ms, 250);
        assert_eq!(config.routes[1].timeout_ms, 30_000);
    }

    #[test]
    fn parses_retry_attempts_and_defaults() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    retry_attempts: 2
    path: /retry
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8002
    path: /
"#,
        )
        .expect("retry config should parse");

        assert_eq!(config.routes[0].retry_attempts, 2);
        assert_eq!(config.routes[1].retry_attempts, 0);
    }

    #[test]
    fn parses_retry_statuses_and_backoff() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    retry_attempts: 2
    retry_statuses: [503, 504]
    retry_backoff_ms: 25
    path: /retry
"#,
        )
        .expect("retry policy config should parse");

        assert_eq!(config.routes[0].retry_statuses.as_ref(), &[503, 504]);
        assert_eq!(config.routes[0].retry_backoff_ms, 25);
    }

    #[test]
    fn parses_route_rate_limit() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    rate_limit:
      requests: 2
      window_ms: 1000
      key_header: x-api-key
    path: /api
"#,
        )
        .expect("rate limit config should parse");

        let rate_limit = config.routes[0].rate_limit.as_ref().expect("rate limit");
        assert_eq!(rate_limit.requests, 2);
        assert_eq!(rate_limit.window_ms, 1000);
        assert_eq!(rate_limit.key_header, "x-api-key");
    }

    #[test]
    fn parses_route_hosts_and_methods() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    hosts: [api.example.com]
    methods: [GET, POST]
    path: /api
"#,
        )
        .expect("predicate config should parse");

        assert_eq!(config.routes[0].hosts.len(), 1);
        assert_eq!(config.routes[0].hosts[0], "api.example.com");
        assert_eq!(config.routes[0].methods.len(), 2);
        assert_eq!(config.routes[0].methods[1], "POST");
    }

    #[test]
    fn parses_route_rewrite_prefix() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    rewrite_prefix: /internal
    path: /api
"#,
        )
        .expect("rewrite config should parse");

        assert_eq!(
            config.routes[0].rewrite_prefix.as_deref(),
            Some("/internal")
        );
    }

    #[test]
    fn parses_route_headers_and_queries() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    headers:
      x-tenant: acme
    queries:
      region: kr
    path: /search
"#,
        )
        .expect("header/query config should parse");

        assert_eq!(
            config.routes[0].headers.get("x-tenant"),
            Some(&"acme".to_string())
        );
        assert_eq!(
            config.routes[0].queries.get("region"),
            Some(&"kr".to_string())
        );
    }

    #[test]
    fn parses_request_and_response_header_filters() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    request_header_filter:
      remove: [x-remove-me]
      set:
        x-added-by-gateway: enabled
    response_header_filter:
      remove: [server]
      set:
        x-gateway: hyper-gateway
    path: /
"#,
        )
        .expect("header filter config should parse");

        assert_eq!(
            config.routes[0].request_header_filter.remove.as_ref(),
            &["x-remove-me".to_string()]
        );
        assert_eq!(
            config.routes[0].response_header_filter.set.get("x-gateway"),
            Some(&"hyper-gateway".to_string())
        );
    }

    #[test]
    fn parses_route_circuit_breaker() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    circuit_breaker:
      failure_threshold: 2
      open_ms: 250
    path: /
"#,
        )
        .expect("circuit breaker config should parse");

        let circuit_breaker = config.routes[0]
            .circuit_breaker
            .as_ref()
            .expect("circuit breaker config");
        assert_eq!(circuit_breaker.failure_threshold, 2);
        assert_eq!(circuit_breaker.open_ms, 250);
    }

    #[test]
    fn parses_additional_upstreams() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
routes:
  - scheme: http
    protocol: http1
    authority:
      host: 127.0.0.1
      port: 8001
    upstreams:
      - host: 127.0.0.1
        port: 8002
    path: /
"#,
        )
        .expect("upstreams config should parse");

        assert_eq!(config.routes[0].upstreams.len(), 1);
        assert_eq!(config.routes[0].upstreams[0].port, "8002");
    }

    #[test]
    fn rejects_unknown_protocol() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
route:
  - scheme: http
    protocol: bogus
    authority:
      host: 127.0.0.1
      port: 8001
    path: /
"#,
        );

        assert!(config.is_err());
    }

    #[test]
    fn rejects_missing_protocol() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
route:
  - scheme: http
    authority:
      host: 127.0.0.1
      port: 8001
    path: /
"#,
        );

        assert!(config.is_err());
    }

    #[test]
    fn prefers_longest_matching_route_prefix() {
        let routes: Arc<[Route]> = Arc::from([
            Route {
                scheme: "http".to_string(),
                protocol: Protocol::Http1,
                authority: Authority {
                    host: "127.0.0.1".to_string(),
                    port: "8001".to_string(),
                },
                upstreams: Arc::from([]),
                tls: RouteTlsConfig::default(),
                timeout_ms: default_route_timeout_ms(),
                retry_attempts: default_route_retry_attempts(),
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
                hosts: Arc::from([]),
                methods: Arc::from([]),
                headers: std::collections::BTreeMap::new(),
                queries: std::collections::BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: HeaderFilterConfig::default(),
                response_header_filter: HeaderFilterConfig::default(),
                path: "/".to_string(),
            },
            Route {
                scheme: "http".to_string(),
                protocol: Protocol::H2c,
                authority: Authority {
                    host: "127.0.0.1".to_string(),
                    port: "8002".to_string(),
                },
                upstreams: Arc::from([]),
                tls: RouteTlsConfig::default(),
                timeout_ms: default_route_timeout_ms(),
                retry_attempts: default_route_retry_attempts(),
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
                hosts: Arc::from([]),
                methods: Arc::from([]),
                headers: std::collections::BTreeMap::new(),
                queries: std::collections::BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: HeaderFilterConfig::default(),
                response_header_filter: HeaderFilterConfig::default(),
                path: "/h2".to_string(),
            },
        ]);

        let route = get_route("/h2/resource", &routes).expect("route should exist");

        assert_eq!(route.path, "/h2");
        assert_eq!(route.protocol, Protocol::H2c);
    }
}
