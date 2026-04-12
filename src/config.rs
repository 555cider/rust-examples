use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, sync::Arc};

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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen_addr(),
            tls: None,
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
    pub tls: RouteTlsConfig,
    #[serde(default = "default_route_timeout_ms")]
    pub timeout_ms: u64,
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Authority {
    pub host: String,
    pub port: String,
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

fn default_listen_addr() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_route_timeout_ms() -> u64 {
    30_000
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
                path: "/h2".to_string(),
            },
        ]);

        let route = get_route("/h2/resource", &routes).expect("route should exist");

        assert_eq!(route.path, "/h2");
        assert_eq!(route.protocol, Protocol::H2c);
    }
}
