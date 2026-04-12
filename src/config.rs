use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, convert::TryFrom, fs::File, io::Read, sync::Arc};

#[derive(Debug, Serialize, Clone, Deserialize)]
#[serde(try_from = "RawGatewayConfig")]
pub struct GatewayConfig {
    pub server: ServerConfig,
    pub routes: Arc<[Route]>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen: String,
    #[serde(default)]
    pub tls: Option<ServerTlsConfig>,
    #[serde(default)]
    pub discovery: DiscoveryConfig,
    #[serde(default)]
    pub management: ManagementConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen_addr(),
            tls: None,
            discovery: DiscoveryConfig::default(),
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
    pub discovery: Option<RouteDiscoveryConfig>,
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
    #[serde(deserialize_with = "deserialize_string_like")]
    pub port: String,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct DiscoveryConfig {
    #[serde(default = "default_discovery_refresh_ms")]
    pub refresh_ms: u64,
    #[serde(default)]
    pub consul: Option<ConsulDiscoveryConfig>,
    #[serde(default)]
    pub eureka: Option<EurekaDiscoveryConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct ConsulDiscoveryConfig {
    pub address: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default = "default_consul_passing_only")]
    pub passing_only: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct EurekaDiscoveryConfig {
    pub service_url: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub prefer_ip_address: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct RouteDiscoveryConfig {
    pub service: String,
    #[serde(default)]
    pub port_override: Option<String>,
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
    pub listen: Option<String>,
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

fn default_discovery_refresh_ms() -> u64 {
    5_000
}

fn default_consul_passing_only() -> bool {
    true
}

fn deserialize_string_like<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringLikeVisitor;

    impl<'de> serde::de::Visitor<'de> for StringLikeVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a string or integer-like scalar")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }
    }

    deserializer.deserialize_any(StringLikeVisitor)
}

pub fn load_config(path: &str) -> GatewayConfig {
    let mut contents: String = String::new();
    let mut file: File = File::open(path).expect("Failed to open the configuration file!");
    file.read_to_string(&mut contents)
        .expect("Failed to read the configuration file!");
    serde_yaml::from_str(&contents).expect("Failed to parse the configuration file!")
}

pub fn resolve_config_path() -> Result<String, String> {
    resolve_config_path_from(std::env::args(), std::env::var("GATEWAY_CONFIG").ok())
}

pub fn resolve_config_path_from<I, S>(
    args: I,
    env_config_path: Option<String>,
) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<String>>();
    let cli_args = args.into_iter().skip(1).collect::<Vec<_>>();

    match cli_args.as_slice() {
        [] => Ok(env_config_path.unwrap_or_else(|| "config.yaml".to_string())),
        [flag] if flag == "--config" || flag == "-c" => {
            Err("usage: hyper-gateway [--config <path> | <path>]".to_string())
        }
        [path] => Ok(path.clone()),
        [flag, path] if flag == "--config" || flag == "-c" => Ok(path.clone()),
        _ => Err("usage: hyper-gateway [--config <path> | <path>]".to_string()),
    }
}

pub fn get_route<'a>(path: &str, route: &'a [Route]) -> Option<&'a Route> {
    route
        .iter()
        .filter(|candidate| path.starts_with(&candidate.path))
        .max_by_key(|candidate| candidate.path.len())
}

#[derive(Debug, Default, Deserialize)]
struct RawGatewayConfig {
    #[serde(default)]
    server: ServerConfig,
    #[serde(default)]
    routes: Vec<serde_yaml::Value>,
    #[serde(default)]
    route: Vec<serde_yaml::Value>,
    #[serde(default)]
    spring: RawSpringConfig,
}

#[derive(Debug, Default, Deserialize)]
struct RawSpringConfig {
    #[serde(default)]
    cloud: RawSpringCloudConfig,
}

#[derive(Debug, Default, Deserialize)]
struct RawSpringCloudConfig {
    #[serde(default)]
    gateway: RawSpringGatewayConfig,
}

#[derive(Debug, Default, Deserialize)]
struct RawSpringGatewayConfig {
    #[serde(default)]
    routes: Vec<ScgRouteDefinition>,
}

#[derive(Debug, Default, Deserialize)]
struct ScgRouteDefinition {
    #[serde(default)]
    id: Option<String>,
    uri: String,
    #[serde(default)]
    predicates: Vec<String>,
    #[serde(default)]
    filters: Vec<String>,
    #[serde(default)]
    metadata: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Default)]
struct ScgMetadata {
    scheme: Option<String>,
    protocol: Option<Protocol>,
    port: Option<u16>,
    timeout_ms: Option<u64>,
    retry_attempts: Option<usize>,
    retry_statuses: Option<Arc<[u16]>>,
    retry_backoff_ms: Option<u64>,
}

impl TryFrom<RawGatewayConfig> for GatewayConfig {
    type Error = String;

    fn try_from(raw: RawGatewayConfig) -> Result<Self, Self::Error> {
        let mut routes = Vec::new();

        for route in raw.route.into_iter().chain(raw.routes) {
            routes.push(parse_route_definition(route)?);
        }
        for route in raw.spring.cloud.gateway.routes {
            routes.push(convert_scg_route(route)?);
        }

        Ok(Self {
            server: raw.server,
            routes: Arc::from(routes),
        })
    }
}

fn parse_route_definition(value: serde_yaml::Value) -> Result<Route, String> {
    if let Ok(route) = serde_yaml::from_value::<Route>(value.clone()) {
        return Ok(route);
    }
    if let Ok(route) = serde_yaml::from_value::<ScgRouteDefinition>(value.clone()) {
        return convert_scg_route(route);
    }

    let legacy_err = serde_yaml::from_value::<Route>(value.clone())
        .map(|_| ())
        .unwrap_err();
    let scg_err = serde_yaml::from_value::<ScgRouteDefinition>(value)
        .map(|_| ())
        .unwrap_err();

    Err(format!(
        "route definition did not match the legacy or scg-style schema (legacy: {}; scg: {})",
        legacy_err, scg_err
    ))
}

fn convert_scg_route(route: ScgRouteDefinition) -> Result<Route, String> {
    let metadata = parse_scg_metadata(&route.metadata)?;
    let parsed_uri = parse_scg_uri(route.id.as_deref(), &route.uri, &metadata)?;

    let mut path = "/".to_string();
    let mut hosts = Vec::new();
    let mut methods = Vec::new();
    let mut headers = BTreeMap::new();
    let mut queries = BTreeMap::new();

    for predicate in &route.predicates {
        let (name, args) = split_scg_assignment("predicate", predicate)?;
        match name.to_ascii_lowercase().as_str() {
            "path" => {
                if path != "/" {
                    return Err(format!(
                        "multiple scg Path predicates are not supported for route '{}'",
                        route_name(route.id.as_deref(), &route.uri)
                    ));
                }
                path = parse_scg_path_predicate(args)?;
            }
            "host" => {
                for host in parse_csv(args) {
                    if host.contains('*') {
                        return Err(format!(
                            "unsupported scg predicate 'Host' value '{}' for route '{}'",
                            host,
                            route_name(route.id.as_deref(), &route.uri)
                        ));
                    }
                    hosts.push(host.to_string());
                }
            }
            "method" => {
                methods.extend(parse_csv(args).into_iter().map(str::to_string));
            }
            "header" => {
                let (header_name, header_value) = split_scg_pair("Header", args)?;
                headers.insert(header_name.to_string(), header_value.to_string());
            }
            "query" => {
                let (query_name, query_value) = split_scg_pair("Query", args)?;
                queries.insert(query_name.to_string(), query_value.to_string());
            }
            other => {
                return Err(format!(
                    "unsupported scg predicate '{}' for route '{}'",
                    other,
                    route_name(route.id.as_deref(), &route.uri)
                ));
            }
        }
    }

    let mut rewrite_prefix = None;
    let mut request_header_remove = Vec::new();
    let mut request_header_set = BTreeMap::new();
    let mut response_header_remove = Vec::new();
    let mut response_header_set = BTreeMap::new();

    for filter in &route.filters {
        let (name, args) = split_scg_assignment("filter", filter)?;
        match name.to_ascii_lowercase().as_str() {
            "stripprefix" => {
                let segments = args.trim().parse::<usize>().map_err(|err| {
                    format!(
                        "invalid scg StripPrefix value '{}' for route '{}': {}",
                        args,
                        route_name(route.id.as_deref(), &route.uri),
                        err
                    )
                })?;
                let rewritten = strip_path_segments(&path, segments)?;
                rewrite_prefix = Some(rewritten);
            }
            "prefixpath" => {
                let prefix = normalize_prefix_path(args)?;
                let current = rewrite_prefix.clone().unwrap_or_else(|| path.clone());
                rewrite_prefix = Some(join_path_prefix(prefix.as_str(), current.as_str()));
            }
            "setrequestheader" => {
                let (header_name, header_value) = split_scg_pair("SetRequestHeader", args)?;
                request_header_set.insert(header_name.to_string(), header_value.to_string());
            }
            "removerequestheader" => {
                request_header_remove.push(args.trim().to_string());
            }
            "setresponseheader" => {
                let (header_name, header_value) = split_scg_pair("SetResponseHeader", args)?;
                response_header_set.insert(header_name.to_string(), header_value.to_string());
            }
            "removeresponseheader" => {
                response_header_remove.push(args.trim().to_string());
            }
            other => {
                return Err(format!(
                    "unsupported scg filter '{}' for route '{}'",
                    other,
                    route_name(route.id.as_deref(), &route.uri)
                ));
            }
        }
    }

    Ok(Route {
        scheme: parsed_uri.scheme,
        protocol: parsed_uri.protocol,
        authority: parsed_uri.authority,
        discovery: parsed_uri.discovery,
        upstreams: Arc::from([]),
        hosts: Arc::from(hosts),
        methods: Arc::from(methods),
        headers,
        queries,
        tls: RouteTlsConfig::default(),
        timeout_ms: metadata.timeout_ms.unwrap_or_else(default_route_timeout_ms),
        retry_attempts: metadata
            .retry_attempts
            .unwrap_or_else(default_route_retry_attempts),
        retry_statuses: metadata.retry_statuses.unwrap_or_else(|| Arc::from([])),
        retry_backoff_ms: metadata
            .retry_backoff_ms
            .unwrap_or_else(default_route_retry_backoff_ms),
        circuit_breaker: None,
        rate_limit: None,
        rewrite_prefix,
        request_header_filter: HeaderFilterConfig {
            set: request_header_set,
            remove: Arc::from(request_header_remove),
        },
        response_header_filter: HeaderFilterConfig {
            set: response_header_set,
            remove: Arc::from(response_header_remove),
        },
        path,
    })
}

fn route_name<'a>(id: Option<&'a str>, uri: &'a str) -> String {
    id.map(str::to_string)
        .unwrap_or_else(|| format!("uri={}", uri))
}

fn split_scg_assignment<'a>(kind: &str, value: &'a str) -> Result<(&'a str, &'a str), String> {
    value.split_once('=').map_or_else(
        || Err(format!("invalid scg {} '{}'", kind, value)),
        |(name, args)| Ok((name.trim(), args.trim())),
    )
}

fn split_scg_pair<'a>(kind: &str, value: &'a str) -> Result<(&'a str, &'a str), String> {
    value.split_once(',').map_or_else(
        || Err(format!("invalid scg {} '{}'", kind, value)),
        |(name, val)| Ok((name.trim(), val.trim())),
    )
}

fn parse_csv(value: &str) -> Vec<&str> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect()
}

fn parse_scg_path_predicate(value: &str) -> Result<String, String> {
    let patterns = parse_csv(value);
    if patterns.len() != 1 {
        return Err(format!(
            "multiple scg Path patterns are not supported: '{}'",
            value
        ));
    }

    let pattern = patterns[0];
    if !pattern.starts_with('/') {
        return Err(format!(
            "scg Path predicate must start with '/': '{}'",
            pattern
        ));
    }
    if pattern.contains('{') {
        return Err(format!(
            "template scg Path predicates are not supported: '{}'",
            pattern
        ));
    }

    if let Some(prefix) = pattern.strip_suffix("/**") {
        return Ok(if prefix.is_empty() {
            "/".to_string()
        } else {
            prefix.to_string()
        });
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return Ok(if prefix.is_empty() {
            "/".to_string()
        } else {
            prefix.to_string()
        });
    }
    if pattern.contains('*') {
        return Err(format!(
            "unsupported wildcard scg Path predicate '{}'",
            pattern
        ));
    }

    Ok(pattern.to_string())
}

fn normalize_prefix_path(value: &str) -> Result<String, String> {
    let prefix = value.trim();
    if !prefix.starts_with('/') {
        return Err(format!("PrefixPath must start with '/': '{}'", prefix));
    }
    Ok(prefix.to_string())
}

fn strip_path_segments(path: &str, segments: usize) -> Result<String, String> {
    let path_segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments > path_segments.len() {
        return Err(format!(
            "StripPrefix={} exceeds route path segment count for '{}'",
            segments, path
        ));
    }

    let remaining = &path_segments[segments..];
    if remaining.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", remaining.join("/")))
    }
}

fn join_path_prefix(prefix: &str, remainder: &str) -> String {
    if remainder.is_empty() {
        return prefix.to_string();
    }

    match (prefix.ends_with('/'), remainder.starts_with('/')) {
        (true, true) => format!("{}{}", prefix, &remainder[1..]),
        (false, false) => format!("{}/{}", prefix, remainder),
        _ => format!("{}{}", prefix, remainder),
    }
}

struct ParsedScgUri {
    scheme: String,
    protocol: Protocol,
    authority: Authority,
    discovery: Option<RouteDiscoveryConfig>,
}

fn parse_scg_uri(
    route_id: Option<&str>,
    uri: &str,
    metadata: &ScgMetadata,
) -> Result<ParsedScgUri, String> {
    let parsed: hyper::Uri = uri.parse().map_err(|err| {
        format!(
            "invalid scg uri '{}' for route '{}': {}",
            uri,
            route_name(route_id, uri),
            err
        )
    })?;
    let scheme = parsed
        .scheme_str()
        .ok_or_else(|| format!("scg uri '{}' is missing a scheme", uri))?;
    let authority = parsed
        .authority()
        .ok_or_else(|| format!("scg uri '{}' is missing an authority", uri))?;

    let host = authority.host().to_string();
    let port = authority
        .port_u16()
        .or(metadata.port)
        .or_else(|| default_port_for_scheme(scheme, metadata.scheme.as_deref()))
        .ok_or_else(|| {
            format!(
                "scg uri '{}' for route '{}' is missing a port",
                uri,
                route_name(route_id, uri)
            )
        })?
        .to_string();

    match scheme {
        "http" => Ok(ParsedScgUri {
            scheme: "http".to_string(),
            protocol: metadata.protocol.clone().unwrap_or(Protocol::Http1),
            authority: Authority { host, port },
            discovery: None,
        }),
        "https" => Ok(ParsedScgUri {
            scheme: "https".to_string(),
            protocol: metadata.protocol.clone().unwrap_or(Protocol::Http1),
            authority: Authority { host, port },
            discovery: None,
        }),
        "h2c" => Ok(ParsedScgUri {
            scheme: "http".to_string(),
            protocol: Protocol::H2c,
            authority: Authority { host, port },
            discovery: None,
        }),
        "lb" => Ok(ParsedScgUri {
            scheme: metadata
                .scheme
                .clone()
                .unwrap_or_else(|| "http".to_string()),
            protocol: metadata.protocol.clone().unwrap_or(Protocol::Http1),
            authority: Authority {
                host: host.clone(),
                port: port.clone(),
            },
            discovery: Some(RouteDiscoveryConfig {
                service: host,
                port_override: authority
                    .port_u16()
                    .or(metadata.port)
                    .map(|value| value.to_string()),
            }),
        }),
        other => Err(format!(
            "unsupported scg uri scheme '{}' for route '{}'",
            other,
            route_name(route_id, uri)
        )),
    }
}

fn default_port_for_scheme(scheme: &str, metadata_scheme: Option<&str>) -> Option<u16> {
    match scheme {
        "http" | "h2c" => Some(80),
        "https" => Some(443),
        "lb" => match metadata_scheme.unwrap_or("http") {
            "http" => Some(80),
            "https" => Some(443),
            _ => None,
        },
        _ => None,
    }
}

fn parse_scg_metadata(values: &BTreeMap<String, serde_yaml::Value>) -> Result<ScgMetadata, String> {
    Ok(ScgMetadata {
        scheme: metadata_string(values, "scheme")?,
        protocol: metadata_protocol(values, "protocol")?,
        port: metadata_u16(values, "port")?,
        timeout_ms: metadata_u64(values, "timeoutMs")?,
        retry_attempts: metadata_usize(values, "retryAttempts")?,
        retry_statuses: metadata_u16_list(values, "retryStatuses")?,
        retry_backoff_ms: metadata_u64(values, "retryBackoffMs")?,
    })
}

fn metadata_string(
    values: &BTreeMap<String, serde_yaml::Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match values.get(key) {
        None => Ok(None),
        Some(serde_yaml::Value::String(value)) => Ok(Some(value.clone())),
        Some(value) => Err(format!(
            "metadata '{}' must be a string, got {}",
            key,
            metadata_type(value)
        )),
    }
}

fn metadata_protocol(
    values: &BTreeMap<String, serde_yaml::Value>,
    key: &str,
) -> Result<Option<Protocol>, String> {
    match metadata_string(values, key)? {
        None => Ok(None),
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "http1" => Ok(Some(Protocol::Http1)),
            "h2c" => Ok(Some(Protocol::H2c)),
            "h2" => Ok(Some(Protocol::H2)),
            _ => Err(format!("unsupported metadata protocol '{}'", value)),
        },
    }
}

fn metadata_u64(
    values: &BTreeMap<String, serde_yaml::Value>,
    key: &str,
) -> Result<Option<u64>, String> {
    match values.get(key) {
        None => Ok(None),
        Some(serde_yaml::Value::Number(value)) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("metadata '{}' must be an unsigned integer", key)),
        Some(serde_yaml::Value::String(value)) => value
            .parse::<u64>()
            .map(Some)
            .map_err(|err| format!("invalid metadata '{}' value '{}': {}", key, value, err)),
        Some(value) => Err(format!(
            "metadata '{}' must be an unsigned integer, got {}",
            key,
            metadata_type(value)
        )),
    }
}

fn metadata_usize(
    values: &BTreeMap<String, serde_yaml::Value>,
    key: &str,
) -> Result<Option<usize>, String> {
    metadata_u64(values, key)?
        .map(|value| {
            usize::try_from(value)
                .map_err(|_| format!("metadata '{}' value '{}' does not fit in usize", key, value))
        })
        .transpose()
}

fn metadata_u16(
    values: &BTreeMap<String, serde_yaml::Value>,
    key: &str,
) -> Result<Option<u16>, String> {
    metadata_u64(values, key)?
        .map(|value| {
            u16::try_from(value)
                .map_err(|_| format!("metadata '{}' value '{}' does not fit in u16", key, value))
        })
        .transpose()
}

fn metadata_u16_list(
    values: &BTreeMap<String, serde_yaml::Value>,
    key: &str,
) -> Result<Option<Arc<[u16]>>, String> {
    let Some(value) = values.get(key) else {
        return Ok(None);
    };

    let list = match value {
        serde_yaml::Value::Sequence(sequence) => {
            let mut result = Vec::with_capacity(sequence.len());
            for item in sequence {
                match item {
                    serde_yaml::Value::Number(number) => {
                        let value = number.as_u64().ok_or_else(|| {
                            format!("metadata '{}' entries must be unsigned integers", key)
                        })?;
                        result.push(u16::try_from(value).map_err(|_| {
                            format!("metadata '{}' entry '{}' does not fit in u16", key, value)
                        })?);
                    }
                    _ => {
                        return Err(format!(
                            "metadata '{}' entries must be unsigned integers",
                            key
                        ));
                    }
                }
            }
            result
        }
        serde_yaml::Value::String(string) => parse_csv(string)
            .into_iter()
            .map(|item| {
                item.parse::<u16>()
                    .map_err(|err| format!("invalid metadata '{}' entry '{}': {}", key, item, err))
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(format!(
                "metadata '{}' must be a sequence or comma-delimited string",
                key
            ));
        }
    };

    Ok(Some(Arc::from(list)))
}

fn metadata_type(value: &serde_yaml::Value) -> &'static str {
    match value {
        serde_yaml::Value::Null => "null",
        serde_yaml::Value::Bool(_) => "bool",
        serde_yaml::Value::Number(_) => "number",
        serde_yaml::Value::String(_) => "string",
        serde_yaml::Value::Sequence(_) => "sequence",
        serde_yaml::Value::Mapping(_) => "mapping",
        serde_yaml::Value::Tagged(_) => "tagged",
    }
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
    fn parses_repository_config_yaml_example() {
        let contents =
            std::fs::read_to_string("config.yaml").expect("repository config.yaml should exist");
        let config =
            serde_yaml::from_str::<GatewayConfig>(&contents).expect("config.yaml should parse");

        assert!(
            !config.routes.is_empty(),
            "config.yaml should define routes"
        );
    }

    #[test]
    fn resolves_default_config_path() {
        let path = resolve_config_path_from(vec!["gateway"], None).expect("default config path");
        assert_eq!(path, "config.yaml");
    }

    #[test]
    fn resolves_config_path_from_environment() {
        let path = resolve_config_path_from(vec!["gateway"], Some("config.local.yaml".to_string()))
            .expect("env config path");
        assert_eq!(path, "config.local.yaml");
    }

    #[test]
    fn resolves_config_path_from_cli_flag() {
        let path = resolve_config_path_from(
            vec!["gateway", "--config", "examples/consul.yaml"],
            Some("config.local.yaml".to_string()),
        )
        .expect("cli config path");
        assert_eq!(path, "examples/consul.yaml");
    }

    #[test]
    fn resolves_config_path_from_positional_argument() {
        let path = resolve_config_path_from(vec!["gateway", "examples/static.yaml"], None)
            .expect("positional config path");
        assert_eq!(path, "examples/static.yaml");
    }

    #[test]
    fn rejects_invalid_config_cli_arguments() {
        let err = resolve_config_path_from(vec!["gateway", "--config"], None)
            .expect_err("invalid cli args should fail");
        assert!(err.contains("usage: hyper-gateway"));
    }

    #[test]
    fn parses_repository_example_configs() {
        for path in [
            "examples/static.yaml",
            "examples/consul.yaml",
            "examples/eureka.yaml",
            "examples/management.yaml",
        ] {
            let contents = std::fs::read_to_string(path)
                .unwrap_or_else(|_| panic!("missing example {}", path));
            let config = serde_yaml::from_str::<GatewayConfig>(&contents)
                .unwrap_or_else(|_| panic!("example {} should parse", path));
            assert!(
                !config.routes.is_empty(),
                "example {} should define routes",
                path
            );
        }
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
    fn parses_server_management_listen() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
server:
  management:
    listen: 127.0.0.1:9000
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    path: /
"#,
        )
        .expect("management listen config should parse");

        assert_eq!(
            config.server.management.listen.as_deref(),
            Some("127.0.0.1:9000")
        );
    }

    #[test]
    fn parses_server_consul_discovery_config() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
server:
  discovery:
    refresh_ms: 2500
    consul:
      address: http://127.0.0.1:8500
      token: consul-token
      passing_only: false
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    path: /
"#,
        )
        .expect("consul discovery config should parse");

        assert_eq!(config.server.discovery.refresh_ms, 2500);
        let consul = config
            .server
            .discovery
            .consul
            .as_ref()
            .expect("consul config");
        assert_eq!(consul.address, "http://127.0.0.1:8500");
        assert_eq!(consul.token.as_deref(), Some("consul-token"));
        assert!(!consul.passing_only);
    }

    #[test]
    fn parses_server_eureka_discovery_config() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
server:
  discovery:
    eureka:
      service_url: http://127.0.0.1:8761/eureka/v2/apps
      username: admin
      password: secret
      prefer_ip_address: true
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    path: /
"#,
        )
        .expect("eureka discovery config should parse");

        let eureka = config
            .server
            .discovery
            .eureka
            .as_ref()
            .expect("eureka config");
        assert_eq!(eureka.service_url, "http://127.0.0.1:8761/eureka/v2/apps");
        assert_eq!(eureka.username.as_deref(), Some("admin"));
        assert_eq!(eureka.password.as_deref(), Some("secret"));
        assert!(eureka.prefer_ip_address);
    }

    #[test]
    fn parses_scg_style_routes() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
server:
  listen: 127.0.0.1:8080
spring:
  cloud:
    gateway:
      routes:
        - id: products
          uri: http://localhost:8001
          predicates:
            - Path=/products/**
            - Host=api.example.com,www.example.com
            - Method=GET,POST
            - Header=x-tenant,acme
            - Query=region,kr
          filters:
            - StripPrefix=1
            - SetRequestHeader=x-added-by-gateway,enabled
            - RemoveResponseHeader=server
          metadata:
            timeoutMs: 250
            retryAttempts: 2
            retryStatuses: [503, 504]
            retryBackoffMs: 25
"#,
        )
        .expect("scg config should parse");

        let route = &config.routes[0];
        assert_eq!(route.scheme, "http");
        assert_eq!(route.protocol, Protocol::Http1);
        assert_eq!(route.authority.host, "localhost");
        assert_eq!(route.authority.port, "8001");
        assert_eq!(route.path, "/products");
        assert_eq!(route.rewrite_prefix.as_deref(), Some("/"));
        assert_eq!(
            route.hosts.as_ref(),
            &["api.example.com", "www.example.com"]
        );
        assert_eq!(route.methods.as_ref(), &["GET", "POST"]);
        assert_eq!(route.headers.get("x-tenant"), Some(&"acme".to_string()));
        assert_eq!(route.queries.get("region"), Some(&"kr".to_string()));
        assert_eq!(route.timeout_ms, 250);
        assert_eq!(route.retry_attempts, 2);
        assert_eq!(route.retry_statuses.as_ref(), &[503, 504]);
        assert_eq!(route.retry_backoff_ms, 25);
        assert_eq!(
            route.request_header_filter.set.get("x-added-by-gateway"),
            Some(&"enabled".to_string())
        );
        assert_eq!(
            route.response_header_filter.remove.as_ref(),
            &["server".to_string()]
        );
    }

    #[test]
    fn parses_scg_lb_uri_with_metadata_overrides() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
spring:
  cloud:
    gateway:
      routes:
        - id: inventory
          uri: lb://inventory.internal
          predicates:
            - Path=/inventory/**
          metadata:
            port: 8443
            scheme: https
            protocol: h2
"#,
        )
        .expect("scg lb route should parse");

        let route = &config.routes[0];
        assert_eq!(route.scheme, "https");
        assert_eq!(route.protocol, Protocol::H2);
        assert_eq!(route.authority.host, "inventory.internal");
        assert_eq!(route.authority.port, "8443");
        assert_eq!(route.path, "/inventory");
        let discovery = route.discovery.as_ref().expect("discovery config");
        assert_eq!(discovery.service, "inventory.internal");
        assert_eq!(discovery.port_override.as_deref(), Some("8443"));
    }

    #[test]
    fn parses_scg_lb_uri_without_port_override() {
        let config = serde_yaml::from_str::<GatewayConfig>(
            r#"
spring:
  cloud:
    gateway:
      routes:
        - id: orders
          uri: lb://orders
          predicates:
            - Path=/orders/**
"#,
        )
        .expect("scg lb route should parse");

        let route = &config.routes[0];
        let discovery = route.discovery.as_ref().expect("discovery config");
        assert_eq!(discovery.service, "orders");
        assert_eq!(discovery.port_override, None);
    }

    #[test]
    fn rejects_unsupported_scg_predicate() {
        let err = serde_yaml::from_str::<GatewayConfig>(
            r#"
spring:
  cloud:
    gateway:
      routes:
        - uri: http://localhost:8001
          predicates:
            - Cookie=session,abc
"#,
        )
        .expect_err("unsupported predicate should fail");

        assert!(err.to_string().contains("unsupported scg predicate"));
    }

    #[test]
    fn rejects_unsupported_scg_filter() {
        let err = serde_yaml::from_str::<GatewayConfig>(
            r#"
spring:
  cloud:
    gateway:
      routes:
        - uri: http://localhost:8001
          predicates:
            - Path=/api/**
          filters:
            - AddRequestHeader=x-test,1
"#,
        )
        .expect_err("unsupported filter should fail");

        assert!(err.to_string().contains("unsupported scg filter"));
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
                discovery: None,
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
                discovery: None,
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
