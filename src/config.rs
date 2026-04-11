use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, sync::Arc};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GatewayConfig {
    pub route: Arc<[Route]>,
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
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Authority {
    pub host: String,
    pub port: String,
}

pub fn load_config(path: &str) -> GatewayConfig {
    let mut contents: String = String::new();
    let mut file: File = File::open(path).expect("Failed to open the configuration file!");
    file.read_to_string(&mut contents)
        .expect("Failed to read the configuration file!");
    serde_yaml::from_str(&contents).expect("Failed to parse the configuration file!")
}

pub fn get_route<'a>(path: &str, route: &'a [Route]) -> Option<&'a Route> {
    route.iter().find(|candidate| path.starts_with(&candidate.path))
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(config.route[0].protocol, Protocol::Http1);
        assert_eq!(config.route[1].protocol, Protocol::H2c);
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
}
