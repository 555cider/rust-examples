use crate::{config, entity, tls};
use reqwest::StatusCode;
use serde::Deserialize;
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::RwLock;

pub type DiscoveryRegistry = Option<Arc<Registry>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredInstance {
    pub host: String,
    pub port: String,
    pub secure_port: Option<String>,
}

pub struct Registry {
    services: RwLock<HashMap<String, Arc<[DiscoveredInstance]>>>,
    cursors: Mutex<HashMap<String, usize>>,
}

impl Registry {
    fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
            cursors: Mutex::new(HashMap::new()),
        }
    }
}

pub async fn init(
    discovery_config: &config::DiscoveryConfig,
    routes: &[config::Route],
) -> Result<DiscoveryRegistry, entity::GatewayError> {
    let service_ids = discovery_service_ids(routes);
    if service_ids.is_empty() {
        return Ok(None);
    }

    let Some(provider) = ProviderClient::from_config(discovery_config)? else {
        return Ok(None);
    };

    let registry = Arc::new(Registry::new());
    refresh_registry(&registry, &provider, &service_ids).await?;

    let refresh_ms = discovery_config.refresh_ms;
    let registry_for_task = registry.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(refresh_ms));
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(err) = refresh_registry(&registry_for_task, &provider, &service_ids).await {
                log::warn!("Discovery refresh failed: {}", err);
            }
        }
    });

    Ok(Some(registry))
}

pub async fn next_authority(
    discovery_registry: &DiscoveryRegistry,
    route: &config::Route,
) -> Result<Option<config::Authority>, entity::GatewayError> {
    let Some(discovery) = route.discovery.as_ref() else {
        return Ok(None);
    };
    let Some(registry) = discovery_registry.as_ref() else {
        return Ok(None);
    };

    let service_key = service_key(discovery.service.as_str());
    let services = registry.services.read().await;
    let Some(instances) = services.get(&service_key) else {
        return Err(entity::GatewayError::Discovery(format!(
            "no discovered instances for service '{}'",
            discovery.service
        )));
    };
    if instances.is_empty() {
        return Err(entity::GatewayError::Discovery(format!(
            "service '{}' has no available instances",
            discovery.service
        )));
    }

    let selected = next_cursor(&registry.cursors, &service_key) % instances.len();
    let instance = &instances[selected];
    Ok(Some(config::Authority {
        host: instance.host.clone(),
        port: discovery.port_override.clone().unwrap_or_else(|| {
            if route.scheme == "https" {
                instance
                    .secure_port
                    .clone()
                    .unwrap_or_else(|| instance.port.clone())
            } else {
                instance.port.clone()
            }
        }),
    }))
}

fn next_cursor(cursors: &Mutex<HashMap<String, usize>>, key: &str) -> usize {
    let mut cursors = cursors.lock().unwrap();
    let cursor = cursors.entry(key.to_string()).or_insert(0);
    let current = *cursor;
    *cursor = cursor.wrapping_add(1);
    current
}

fn discovery_service_ids(routes: &[config::Route]) -> Arc<[String]> {
    let mut service_ids = BTreeSet::new();
    for route in routes {
        if let Some(discovery) = route.discovery.as_ref() {
            service_ids.insert(service_key(discovery.service.as_str()));
        }
    }
    Arc::from(service_ids.into_iter().collect::<Vec<_>>())
}

fn service_key(service: &str) -> String {
    service.to_ascii_lowercase()
}

async fn refresh_registry(
    registry: &Arc<Registry>,
    provider: &ProviderClient,
    service_ids: &[String],
) -> Result<(), entity::GatewayError> {
    let mut snapshot = HashMap::new();
    for service_id in service_ids {
        snapshot.insert(
            service_id.clone(),
            Arc::from(provider.fetch(service_id).await?),
        );
    }

    *registry.services.write().await = snapshot;
    Ok(())
}

enum ProviderClient {
    Consul(reqwest::Client, config::ConsulDiscoveryConfig),
    Eureka(reqwest::Client, config::EurekaDiscoveryConfig),
}

impl ProviderClient {
    fn from_config(
        discovery_config: &config::DiscoveryConfig,
    ) -> Result<Option<Self>, entity::GatewayError> {
        tls::ensure_crypto_provider();
        match (
            discovery_config.consul.as_ref(),
            discovery_config.eureka.as_ref(),
        ) {
            (None, None) => Ok(None),
            (Some(_), Some(_)) => Err(entity::GatewayError::InvalidRouteConfig(
                "server.discovery may configure only one provider at a time".to_string(),
            )),
            (Some(consul), None) => Ok(Some(Self::Consul(reqwest::Client::new(), consul.clone()))),
            (None, Some(eureka)) => Ok(Some(Self::Eureka(reqwest::Client::new(), eureka.clone()))),
        }
    }

    async fn fetch(
        &self,
        service_id: &str,
    ) -> Result<Vec<DiscoveredInstance>, entity::GatewayError> {
        match self {
            Self::Consul(client, config) => {
                fetch_consul_instances(client, config, service_id).await
            }
            Self::Eureka(client, config) => {
                fetch_eureka_instances(client, config, service_id).await
            }
        }
    }
}

async fn fetch_consul_instances(
    client: &reqwest::Client,
    discovery_config: &config::ConsulDiscoveryConfig,
    service_id: &str,
) -> Result<Vec<DiscoveredInstance>, entity::GatewayError> {
    let url = format!(
        "{}/v1/health/service/{}",
        discovery_config.address.trim_end_matches('/'),
        service_id
    );
    let mut request = client.get(url);
    if discovery_config.passing_only {
        request = request.query(&[("passing", "true")]);
    }
    if let Some(token) = discovery_config.token.as_deref() {
        request = request.header("X-Consul-Token", token);
    }

    let response = request.send().await.map_err(reqwest_error)?;
    let response = response.error_for_status().map_err(reqwest_error)?;
    let payload = response
        .json::<Vec<ConsulServiceEntry>>()
        .await
        .map_err(reqwest_error)?;

    Ok(payload
        .into_iter()
        .filter_map(|entry| {
            let host = if !entry.service.address.is_empty() {
                entry.service.address
            } else {
                entry.node.address
            };
            if host.is_empty() {
                return None;
            }
            Some(DiscoveredInstance {
                host,
                port: entry.service.port.to_string(),
                secure_port: None,
            })
        })
        .collect())
}

async fn fetch_eureka_instances(
    client: &reqwest::Client,
    discovery_config: &config::EurekaDiscoveryConfig,
    service_id: &str,
) -> Result<Vec<DiscoveredInstance>, entity::GatewayError> {
    let url = format!(
        "{}/{}",
        discovery_config.service_url.trim_end_matches('/'),
        service_id.to_ascii_uppercase()
    );
    let mut request = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json");
    if let Some(username) = discovery_config.username.as_deref() {
        request = request.basic_auth(username, discovery_config.password.as_deref());
    }

    let response = request.send().await.map_err(reqwest_error)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let response = response.error_for_status().map_err(reqwest_error)?;
    let payload = response
        .json::<EurekaApplicationResponse>()
        .await
        .map_err(reqwest_error)?;

    Ok(payload
        .application
        .map(|application| application.instance.into_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|instance| instance.status.eq_ignore_ascii_case("UP"))
        .filter_map(|instance| {
            let host = if discovery_config.prefer_ip_address && !instance.ip_addr.is_empty() {
                instance.ip_addr.clone()
            } else if !instance.host_name.is_empty() {
                instance.host_name.clone()
            } else {
                instance.ip_addr.clone()
            };
            if host.is_empty() {
                return None;
            }

            instance.selected_port().map(|port| DiscoveredInstance {
                host,
                port,
                secure_port: instance.selected_secure_port(),
            })
        })
        .collect())
}

fn reqwest_error(err: reqwest::Error) -> entity::GatewayError {
    entity::GatewayError::Discovery(err.to_string())
}

#[derive(Deserialize)]
struct ConsulServiceEntry {
    #[serde(rename = "Node")]
    node: ConsulNode,
    #[serde(rename = "Service")]
    service: ConsulService,
}

#[derive(Deserialize)]
struct ConsulNode {
    #[serde(rename = "Address")]
    address: String,
}

#[derive(Deserialize)]
struct ConsulService {
    #[serde(rename = "Address")]
    address: String,
    #[serde(rename = "Port")]
    port: u16,
}

#[derive(Deserialize)]
struct EurekaApplicationResponse {
    application: Option<EurekaApplication>,
}

#[derive(Deserialize)]
struct EurekaApplication {
    instance: OneOrMany<EurekaInstance>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    fn into_vec(self) -> Vec<T> {
        match self {
            Self::One(item) => vec![item],
            Self::Many(items) => items,
        }
    }
}

#[derive(Deserialize)]
struct EurekaInstance {
    #[serde(rename = "hostName", default)]
    host_name: String,
    #[serde(rename = "ipAddr", default)]
    ip_addr: String,
    #[serde(default)]
    status: String,
    port: EurekaPort,
    #[serde(rename = "securePort")]
    secure_port: EurekaPort,
}

impl EurekaInstance {
    fn selected_port(&self) -> Option<String> {
        if self.port.enabled {
            Some(self.port.value.to_string())
        } else if self.port.value > 0 {
            Some(self.port.value.to_string())
        } else if self.secure_port.enabled {
            Some(self.secure_port.value.to_string())
        } else if self.secure_port.value > 0 {
            Some(self.secure_port.value.to_string())
        } else {
            None
        }
    }

    fn selected_secure_port(&self) -> Option<String> {
        if self.secure_port.enabled {
            Some(self.secure_port.value.to_string())
        } else if self.secure_port.value > 0 {
            Some(self.secure_port.value.to_string())
        } else {
            None
        }
    }
}

#[derive(Deserialize)]
struct EurekaPort {
    #[serde(rename = "$")]
    value: u16,
    #[serde(rename = "@enabled", deserialize_with = "deserialize_boolish")]
    enabled: bool,
}

fn deserialize_boolish<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BoolishVisitor;

    impl<'de> serde::de::Visitor<'de> for BoolishVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a bool or boolean-like string")
        }

        fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.eq_ignore_ascii_case("true"))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.eq_ignore_ascii_case("true"))
        }
    }

    deserializer.deserialize_any(BoolishVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_consul_service_entries() {
        let payload = serde_json::from_str::<Vec<ConsulServiceEntry>>(
            r#"[{
              "Node": {"Address":"10.0.0.1"},
              "Service": {"Address":"", "Port":8080}
            }]"#,
        )
        .expect("consul payload");

        assert_eq!(payload[0].node.address, "10.0.0.1");
        assert_eq!(payload[0].service.port, 8080);
    }

    #[test]
    fn parses_eureka_application_entries() {
        let payload = serde_json::from_str::<EurekaApplicationResponse>(
            r#"{
              "application": {
                "instance": {
                  "hostName": "service.local",
                  "ipAddr": "10.0.0.2",
                  "status": "UP",
                  "port": {"$": 8080, "@enabled": "true"},
                  "securePort": {"$": 8443, "@enabled": "false"}
                }
              }
            }"#,
        )
        .expect("eureka payload");

        let instances = payload
            .application
            .expect("application")
            .instance
            .into_vec();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].host_name, "service.local");
        assert_eq!(instances[0].port.value, 8080);
        assert_eq!(instances[0].selected_port().as_deref(), Some("8080"));
        assert_eq!(instances[0].selected_secure_port().as_deref(), Some("8443"));
    }
}
