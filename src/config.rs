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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen_addr(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Http1,
    H2c,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Route {
    pub scheme: String,
    pub protocol: Protocol,
    pub authority: Authority,
    #[serde(default)]
    pub tls: RouteTlsConfig,
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

fn default_listen_addr() -> String {
    "127.0.0.1:8080".to_string()
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
                tls: RouteTlsConfig::default(),
                path: "/".to_string(),
            },
            Route {
                scheme: "http".to_string(),
                protocol: Protocol::H2c,
                authority: Authority {
                    host: "127.0.0.1".to_string(),
                    port: "8002".to_string(),
                },
                tls: RouteTlsConfig::default(),
                path: "/h2".to_string(),
            },
        ]);

        let route = get_route("/h2/resource", &routes).expect("route should exist");

        assert_eq!(route.path, "/h2");
        assert_eq!(route.protocol, Protocol::H2c);
    }
}
