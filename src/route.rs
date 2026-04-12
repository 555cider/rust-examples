use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};

use crate::{config, entity, metrics, throttle, trace, upstream};
use http_body_util::BodyExt;
use hyper::{
    body::{Bytes, Incoming},
    header::{HeaderValue, HOST},
    service::service_fn,
    HeaderMap, Method, Request, Response, Version,
};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder as AutoBuilder,
};
use tokio::io::{AsyncRead, AsyncWrite};

pub async fn run(
    gateway_stream: tokio::net::TcpStream,
    route_config_arr_clone: Arc<[config::Route]>,
    throttle_pool_clone: Arc<Mutex<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>>,
    metrics_collector: metrics::MetricsCollector,
) -> Result<(), entity::GatewayError> {
    serve_connection(
        gateway_stream,
        route_config_arr_clone,
        throttle_pool_clone,
        metrics_collector,
    )
    .await
}

pub async fn serve_connection<T>(
    gateway_stream: T,
    route_config_arr_clone: Arc<[config::Route]>,
    throttle_pool_clone: Arc<Mutex<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>>,
    metrics_collector: metrics::MetricsCollector,
) -> Result<(), entity::GatewayError>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service = service_fn(move |incoming_request: Request<Incoming>| {
        let route_config_arr = route_config_arr_clone.clone();
        let throttle_pool = throttle_pool_clone.clone();
        let metrics_collector = metrics_collector.clone();

        async move {
            let request_path = incoming_request.uri().path().to_string();
            let request_started_at = Instant::now();

            if let Some(response) =
                crate::management::handle(&incoming_request, &route_config_arr, &metrics_collector)?
            {
                if !crate::management::is_metrics_path(&request_path) {
                    metrics::record(
                        &metrics_collector,
                        &request_path,
                        response.status(),
                        request_started_at.elapsed(),
                        None,
                    );
                }
                return Ok::<Response<entity::GatewayBody>, entity::GatewayError>(response);
            }

            let throttle_status = throttle::run(
                &throttle_pool.lock().unwrap().get().unwrap(),
                incoming_request.headers(),
            )
            .unwrap();

            if !throttle_status.is_success() {
                metrics::record(
                    &metrics_collector,
                    "__throttle",
                    throttle_status,
                    request_started_at.elapsed(),
                    None,
                );
                return Ok::<Response<entity::GatewayBody>, entity::GatewayError>(
                    entity::get_gateway_response(throttle_status),
                );
            }

            let response = match route_request(incoming_request, &route_config_arr).await {
                Ok(routed) => {
                    let route_path = routed.route.path.clone();
                    match upstream::send(routed).await {
                        Ok(response) => {
                            metrics::record(
                                &metrics_collector,
                                &route_path,
                                response.status(),
                                request_started_at.elapsed(),
                                None,
                            );
                            Ok(response)
                        }
                        Err(err) => Err((route_path, err)),
                    }
                }
                Err(err) => Err((metrics_route_label_for_error(&err).to_string(), err)),
            };

            match response {
                Ok(response) => Ok(response),
                Err((route_label, err)) => match gateway_response_for_error(&err) {
                    Some(response) => {
                        metrics::record(
                            &metrics_collector,
                            &route_label,
                            response.status(),
                            request_started_at.elapsed(),
                            Some(&err),
                        );
                        Ok(response)
                    }
                    None => Err(err),
                },
            }
        }
    });

    AutoBuilder::new(TokioExecutor::new())
        .serve_connection(TokioIo::new(gateway_stream), service)
        .await
        .map_err(|err| entity::GatewayError::Io(std::io::Error::other(err)))
}

fn metrics_route_label_for_error(error: &entity::GatewayError) -> &'static str {
    match error {
        entity::GatewayError::RouteNotFound(_) => "__unmatched",
        _ => "__gateway_error",
    }
}

pub fn gateway_response_for_error(
    error: &entity::GatewayError,
) -> Option<Response<entity::GatewayBody>> {
    let status = match error {
        entity::GatewayError::RouteNotFound(_) => hyper::http::StatusCode::NOT_FOUND,
        entity::GatewayError::CircuitOpen(_) => hyper::http::StatusCode::SERVICE_UNAVAILABLE,
        entity::GatewayError::Timeout(_) => hyper::http::StatusCode::GATEWAY_TIMEOUT,
        entity::GatewayError::Hyper(_)
        | entity::GatewayError::Io(_)
        | entity::GatewayError::Http(_)
        | entity::GatewayError::InvalidRouteConfig(_) => hyper::http::StatusCode::BAD_GATEWAY,
    };

    Some(entity::get_gateway_response(status))
}

pub fn resolve_route(
    path: &str,
    host: Option<&str>,
    method: &Method,
    headers: &HeaderMap,
    query: Option<&str>,
    route_config_arr: &[config::Route],
) -> Result<config::Route, entity::GatewayError> {
    let mut selected_route: Option<&config::Route> = None;

    for candidate in route_config_arr.iter() {
        if !path.starts_with(&candidate.path) {
            continue;
        }
        if !route_matches_host(candidate, host)
            || !route_matches_method(candidate, method)
            || !route_matches_query(candidate, query)
        {
            continue;
        }
        if !route_matches_headers(candidate, headers)? {
            continue;
        }

        match selected_route {
            Some(current)
                if (candidate.path.len(), route_specificity(candidate))
                    <= (current.path.len(), route_specificity(current)) => {}
            _ => selected_route = Some(candidate),
        }
    }

    selected_route
        .cloned()
        .ok_or_else(|| entity::GatewayError::RouteNotFound(path.to_string()))
}

pub fn build_route_uri(uri: &hyper::Uri, route_config: &config::Route) -> String {
    build_route_uri_for_authority(uri, route_config, &route_config.authority)
}

pub fn build_route_uri_for_authority(
    uri: &hyper::Uri,
    route_config: &config::Route,
    authority: &config::Authority,
) -> String {
    let path_and_query = rewrite_path_and_query(uri, route_config)
        .expect("route uri rewrite should succeed")
        .to_string();

    format!(
        "{}://{}{}",
        route_config.scheme,
        upstream_authority(authority),
        path_and_query
    )
}

pub async fn route_request(
    incoming_request: Request<Incoming>,
    route_config_arr: &[config::Route],
) -> Result<upstream::RoutedRequest, entity::GatewayError> {
    let request_host = request_host(incoming_request.headers(), incoming_request.uri());
    let route_config = resolve_route(
        incoming_request.uri().path(),
        request_host.as_deref(),
        incoming_request.method(),
        incoming_request.headers(),
        incoming_request.uri().query(),
        route_config_arr,
    )?;
    let start_index = select_upstream_start_index(&route_config);
    let request = prepare_request(incoming_request, &route_config).await?;

    Ok(upstream::RoutedRequest {
        route: route_config,
        start_index,
        method: request.method,
        version: request.version,
        headers: request.headers,
        path_and_query: request.path_and_query,
        body: request.body,
    })
}

pub fn request_host(headers: &HeaderMap, uri: &hyper::Uri) -> Option<String> {
    headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| {
            uri.authority()
                .map(|authority| authority.as_str().to_string())
        })
}

pub async fn prepare_request(
    request: Request<Incoming>,
    route_config: &config::Route,
) -> Result<PreparedRequest, entity::GatewayError> {
    let traceparent = extract_traceparent(request.headers());
    let (parts, body) = request.into_parts();
    let body = body.collect().await?.to_bytes();
    let mut headers = parts.headers;
    apply_header_filter(&mut headers, &route_config.request_header_filter)?;
    headers.insert("traceparent", traceparent.as_headervalue());
    let path_and_query = rewrite_path_and_query(&parts.uri, route_config)?;

    Ok(PreparedRequest {
        method: parts.method,
        version: parts.version,
        headers,
        path_and_query,
        body,
    })
}

pub fn apply_header_filter(
    headers: &mut HeaderMap,
    filter: &config::HeaderFilterConfig,
) -> Result<(), entity::GatewayError> {
    for name in filter.remove.iter() {
        let header_name =
            hyper::header::HeaderName::from_bytes(name.as_bytes()).map_err(|err| {
                entity::GatewayError::InvalidRouteConfig(format!(
                    "invalid filter header name '{}': {}",
                    name, err
                ))
            })?;
        headers.remove(header_name);
    }

    for (name, value) in filter.set.iter() {
        let header_name =
            hyper::header::HeaderName::from_bytes(name.as_bytes()).map_err(|err| {
                entity::GatewayError::InvalidRouteConfig(format!(
                    "invalid filter header name '{}': {}",
                    name, err
                ))
            })?;
        let header_value = HeaderValue::from_str(value).map_err(|err| {
            entity::GatewayError::InvalidRouteConfig(format!(
                "invalid filter header value for '{}': {}",
                name, err
            ))
        })?;
        headers.insert(header_name, header_value);
    }

    Ok(())
}

pub fn extract_traceparent(headers: &hyper::HeaderMap) -> trace::Traceparent {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| trace::extract(headers))) {
        Ok(Ok(traceparent)) => traceparent,
        Ok(Err(err)) => {
            log::warn!(
                "Failed to parse trace context. Generating new traceparent: {}",
                err
            );
            trace::new()
        }
        Err(_) => {
            log::warn!("Trace extraction panicked. Generating new traceparent");
            trace::new()
        }
    }
}

pub fn upstream_host_header(
    authority: &config::Authority,
) -> Result<HeaderValue, entity::GatewayError> {
    HeaderValue::from_str(upstream_authority(authority).as_str()).map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "invalid upstream authority '{}:{}': {}",
            authority.host, authority.port, err
        ))
    })
}

pub fn upstream_authority(authority: &config::Authority) -> String {
    match authority.host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(_)) => {
            format!("[{}]:{}", authority.host, authority.port)
        }
        _ => format!("{}:{}", authority.host, authority.port),
    }
}

pub fn select_upstream_start_index(route_config: &config::Route) -> usize {
    next_upstream_cursor(route_balance_key(route_config)) % upstream_pool_size(route_config)
}

pub fn authority_for_attempt<'a>(
    route_config: &'a config::Route,
    start_index: usize,
    attempt: usize,
) -> &'a config::Authority {
    let index = (start_index + attempt) % upstream_pool_size(route_config);

    if index == 0 {
        &route_config.authority
    } else {
        &route_config.upstreams[index - 1]
    }
}

fn upstream_pool_size(route_config: &config::Route) -> usize {
    route_config.upstreams.len() + 1
}

fn route_specificity(route_config: &config::Route) -> usize {
    usize::from(!route_config.hosts.is_empty())
        + usize::from(!route_config.methods.is_empty())
        + usize::from(!route_config.headers.is_empty())
        + usize::from(!route_config.queries.is_empty())
}

fn route_matches_host(route_config: &config::Route, host: Option<&str>) -> bool {
    if route_config.hosts.is_empty() {
        return true;
    }

    let Some(host) = host else {
        return false;
    };
    let request_host = normalize_host(host);

    route_config
        .hosts
        .iter()
        .any(|candidate| normalize_host(candidate) == request_host)
}

fn route_matches_method(route_config: &config::Route, method: &Method) -> bool {
    if route_config.methods.is_empty() {
        return true;
    }

    route_config
        .methods
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(method.as_str()))
}

fn route_matches_headers(
    route_config: &config::Route,
    headers: &HeaderMap,
) -> Result<bool, entity::GatewayError> {
    if route_config.headers.is_empty() {
        return Ok(true);
    }

    for (name, expected_value) in &route_config.headers {
        let header_name =
            hyper::header::HeaderName::from_bytes(name.as_bytes()).map_err(|err| {
                entity::GatewayError::InvalidRouteConfig(format!(
                    "invalid route header name '{}': {}",
                    name, err
                ))
            })?;

        let Some(actual_value) = headers.get(&header_name) else {
            return Ok(false);
        };
        let Ok(actual_value) = actual_value.to_str() else {
            return Ok(false);
        };

        if actual_value != expected_value {
            return Ok(false);
        }
    }

    Ok(true)
}

fn route_matches_query(route_config: &config::Route, query: Option<&str>) -> bool {
    if route_config.queries.is_empty() {
        return true;
    }

    let Some(query) = query else {
        return false;
    };
    let parsed_query = parse_query_pairs(query);

    route_config
        .queries
        .iter()
        .all(|(expected_key, expected_value)| {
            parsed_query.iter().any(|(actual_key, actual_value)| {
                actual_key == expected_key && actual_value == expected_value
            })
        })
}

fn parse_query_pairs(query: &str) -> Vec<(&str, &str)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
        .collect()
}

fn normalize_host(host: &str) -> String {
    let host = host.trim();

    if let Some(rest) = host.strip_prefix('[') {
        if let Some((ipv6, _)) = rest.split_once(']') {
            return ipv6.to_ascii_lowercase();
        }
    }

    match host.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && port.chars().all(|ch| ch.is_ascii_digit()) => {
            host.to_ascii_lowercase()
        }
        _ => host.to_ascii_lowercase(),
    }
}

pub fn rewrite_path_and_query(
    uri: &hyper::Uri,
    route_config: &config::Route,
) -> Result<String, entity::GatewayError> {
    let original_path = uri.path();
    let query = uri
        .query()
        .map(|query| format!("?{}", query))
        .unwrap_or_default();

    let Some(rewrite_prefix) = route_config.rewrite_prefix.as_deref() else {
        return Ok(format!("{}{}", original_path, query));
    };

    if !rewrite_prefix.starts_with('/') {
        return Err(entity::GatewayError::InvalidRouteConfig(format!(
            "rewrite_prefix '{}' must start with '/'",
            rewrite_prefix
        )));
    }

    let Some(remainder) = original_path.strip_prefix(route_config.path.as_str()) else {
        return Err(entity::GatewayError::InvalidRouteConfig(format!(
            "request path '{}' does not match route prefix '{}'",
            original_path, route_config.path
        )));
    };

    Ok(format!(
        "{}{}",
        join_path_prefix(rewrite_prefix, remainder),
        query
    ))
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

fn route_balance_key(route_config: &config::Route) -> String {
    let mut key = format!(
        "{}|{:?}|{}|{}|{}",
        route_config.path,
        route_config.protocol,
        route_config.scheme,
        route_config.authority.host,
        route_config.authority.port
    );

    for authority in route_config.upstreams.iter() {
        key.push('|');
        key.push_str(&authority.host);
        key.push(':');
        key.push_str(&authority.port);
    }

    key
}

fn next_upstream_cursor(route_key: String) -> usize {
    static ROUTE_BALANCERS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

    let balancers = ROUTE_BALANCERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut balancers = balancers.lock().unwrap();
    let cursor = balancers.entry(route_key).or_insert(0);
    let selected = *cursor;
    *cursor = cursor.wrapping_add(1);
    selected
}

pub struct PreparedRequest {
    pub method: Method,
    pub version: Version,
    pub headers: HeaderMap,
    pub path_and_query: String,
    pub body: Bytes,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Authority, HeaderFilterConfig, Protocol, Route, RouteTlsConfig};
    use hyper::http::StatusCode;

    fn sample_route(protocol: Protocol) -> Route {
        Route {
            scheme: "http".to_string(),
            protocol,
            authority: Authority {
                host: "127.0.0.1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: std::collections::BTreeMap::new(),
            queries: std::collections::BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: HeaderFilterConfig::default(),
            response_header_filter: HeaderFilterConfig::default(),
            path: "/".to_string(),
        }
    }

    #[test]
    fn build_route_uri_keeps_query_string() {
        let uri: hyper::Uri = "/product?id=7".parse().expect("uri");
        assert_eq!(
            build_route_uri(&uri, &sample_route(Protocol::Http1)),
            "http://127.0.0.1:9000/product?id=7"
        );
    }

    #[test]
    fn build_route_uri_brackets_ipv6_literals() {
        let uri: hyper::Uri = "/".parse().expect("uri");
        let route = Route {
            scheme: "http".to_string(),
            protocol: Protocol::Http1,
            authority: Authority {
                host: "::1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: std::collections::BTreeMap::new(),
            queries: std::collections::BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: HeaderFilterConfig::default(),
            response_header_filter: HeaderFilterConfig::default(),
            path: "/".to_string(),
        };

        assert_eq!(build_route_uri(&uri, &route), "http://[::1]:9000/");
    }

    #[test]
    fn rewrite_path_and_query_replaces_prefix_and_preserves_query() {
        let route = Route {
            scheme: "http".to_string(),
            protocol: Protocol::Http1,
            authority: Authority {
                host: "127.0.0.1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: std::collections::BTreeMap::new(),
            queries: std::collections::BTreeMap::new(),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            rewrite_prefix: Some("/".to_string()),
            request_header_filter: HeaderFilterConfig::default(),
            response_header_filter: HeaderFilterConfig::default(),
            path: "/api".to_string(),
        };

        let uri: hyper::Uri = "/api/users?id=7".parse().expect("uri");

        assert_eq!(
            rewrite_path_and_query(&uri, &route).expect("rewrite should succeed"),
            "/users?id=7"
        );
    }

    #[test]
    fn rewrite_path_and_query_maps_exact_prefix_to_root() {
        let route = Route {
            scheme: "http".to_string(),
            protocol: Protocol::Http1,
            authority: Authority {
                host: "127.0.0.1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: std::collections::BTreeMap::new(),
            queries: std::collections::BTreeMap::new(),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            rewrite_prefix: Some("/".to_string()),
            request_header_filter: HeaderFilterConfig::default(),
            response_header_filter: HeaderFilterConfig::default(),
            path: "/api".to_string(),
        };

        let uri: hyper::Uri = "/api".parse().expect("uri");

        assert_eq!(
            rewrite_path_and_query(&uri, &route).expect("rewrite should succeed"),
            "/"
        );
    }

    #[test]
    fn resolve_route_preserves_protocol() {
        let routes: Arc<[Route]> = Arc::from([sample_route(Protocol::H2c)]);

        let route = resolve_route("/", None, &Method::GET, &HeaderMap::new(), None, &routes)
            .expect("route should exist");
        assert_eq!(route.protocol, Protocol::H2c);
    }

    #[test]
    fn resolve_route_matches_host_and_method_predicates() {
        let routes: Arc<[Route]> = Arc::from([Route {
            scheme: "http".to_string(),
            protocol: Protocol::Http1,
            authority: Authority {
                host: "127.0.0.1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from(["api.example.com".to_string()]),
            methods: Arc::from(["POST".to_string()]),
            headers: std::collections::BTreeMap::new(),
            queries: std::collections::BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: HeaderFilterConfig::default(),
            response_header_filter: HeaderFilterConfig::default(),
            path: "/api".to_string(),
        }]);

        let route = resolve_route(
            "/api/users",
            Some("api.example.com:8443"),
            &Method::POST,
            &HeaderMap::new(),
            None,
            &routes,
        )
        .expect("route should match host and method");

        assert_eq!(route.path, "/api");
    }

    #[test]
    fn resolve_route_rejects_non_matching_host_predicate() {
        let routes: Arc<[Route]> = Arc::from([Route {
            scheme: "http".to_string(),
            protocol: Protocol::Http1,
            authority: Authority {
                host: "127.0.0.1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from(["api.example.com".to_string()]),
            methods: Arc::from([]),
            headers: std::collections::BTreeMap::new(),
            queries: std::collections::BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: HeaderFilterConfig::default(),
            response_header_filter: HeaderFilterConfig::default(),
            path: "/".to_string(),
        }]);

        let err = resolve_route(
            "/",
            Some("www.example.com"),
            &Method::GET,
            &HeaderMap::new(),
            None,
            &routes,
        )
        .expect_err("route should not match wrong host");

        assert!(matches!(err, entity::GatewayError::RouteNotFound(_)));
    }

    #[test]
    fn resolve_route_matches_exact_header_predicates() {
        let routes: Arc<[Route]> = Arc::from([Route {
            scheme: "http".to_string(),
            protocol: Protocol::Http1,
            authority: Authority {
                host: "127.0.0.1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: std::collections::BTreeMap::from([(
                "x-tenant".to_string(),
                "acme".to_string(),
            )]),
            queries: std::collections::BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: HeaderFilterConfig::default(),
            response_header_filter: HeaderFilterConfig::default(),
            path: "/".to_string(),
        }]);

        let mut headers = HeaderMap::new();
        headers.insert("x-tenant", HeaderValue::from_static("acme"));

        let route = resolve_route("/", None, &Method::GET, &headers, None, &routes)
            .expect("route should match header predicate");

        assert_eq!(route.path, "/");
    }

    #[test]
    fn resolve_route_matches_exact_query_predicates() {
        let routes: Arc<[Route]> = Arc::from([Route {
            scheme: "http".to_string(),
            protocol: Protocol::Http1,
            authority: Authority {
                host: "127.0.0.1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: std::collections::BTreeMap::new(),
            queries: std::collections::BTreeMap::from([("region".to_string(), "kr".to_string())]),
            rewrite_prefix: None,
            request_header_filter: HeaderFilterConfig::default(),
            response_header_filter: HeaderFilterConfig::default(),
            path: "/search".to_string(),
        }]);

        let route = resolve_route(
            "/search",
            None,
            &Method::GET,
            &HeaderMap::new(),
            Some("region=kr&lang=ko"),
            &routes,
        )
        .expect("route should match query predicate");

        assert_eq!(route.path, "/search");
    }

    #[test]
    fn apply_header_filter_removes_then_sets_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-remove-me", HeaderValue::from_static("client"));
        headers.insert("x-added-by-gateway", HeaderValue::from_static("old"));
        let filter = HeaderFilterConfig {
            set: std::collections::BTreeMap::from([(
                "x-added-by-gateway".to_string(),
                "enabled".to_string(),
            )]),
            remove: Arc::from(["x-remove-me".to_string()]),
        };

        apply_header_filter(&mut headers, &filter).expect("filter should apply");

        assert!(!headers.contains_key("x-remove-me"));
        assert_eq!(
            headers.get("x-added-by-gateway").expect("set header"),
            "enabled"
        );
    }

    #[test]
    fn route_not_found_maps_to_not_found_response() {
        let response = gateway_response_for_error(&entity::GatewayError::RouteNotFound(
            "/missing".to_string(),
        ))
        .expect("not found should map to response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn circuit_open_maps_to_service_unavailable_response() {
        let response =
            gateway_response_for_error(&entity::GatewayError::CircuitOpen("/".to_string()))
                .expect("circuit open should map to response");

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn upstream_host_header_uses_selected_authority() {
        let header =
            upstream_host_header(&sample_route(Protocol::Http1).authority).expect("host header");

        assert_eq!(header, "127.0.0.1:9000");
    }

    #[test]
    fn upstream_host_header_brackets_ipv6_literals() {
        let route = Route {
            scheme: "http".to_string(),
            protocol: Protocol::Http1,
            authority: Authority {
                host: "::1".to_string(),
                port: "9000".to_string(),
            },
            upstreams: Arc::from([]),
            tls: RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: std::collections::BTreeMap::new(),
            queries: std::collections::BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: HeaderFilterConfig::default(),
            response_header_filter: HeaderFilterConfig::default(),
            path: "/".to_string(),
        };

        let header = upstream_host_header(&route.authority).expect("host header");

        assert_eq!(header, "[::1]:9000");
    }

    #[test]
    fn extract_traceparent_gracefully_falls_back_on_invalid_trace_context() {
        let mut headers = hyper::HeaderMap::new();
        headers.insert(
            "tracestate",
            hyper::header::HeaderValue::from_static("vendor=value"),
        );
        headers.insert(
            "traceparent",
            hyper::header::HeaderValue::from_static(
                "00-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz-0000000000000000-00",
            ),
        );

        let traceparent = extract_traceparent(&headers);

        assert_eq!(traceparent.as_string().len(), 55);
    }

    #[test]
    fn extract_traceparent_preserves_valid_traceparent_without_tracestate() {
        let mut headers = hyper::HeaderMap::new();
        headers.insert(
            "traceparent",
            hyper::header::HeaderValue::from_static(
                "00-00000000000000000000000000000001-0000000000000002-01",
            ),
        );

        let traceparent = extract_traceparent(&headers);

        assert_eq!(
            traceparent.as_string(),
            "00-00000000000000000000000000000001-0000000000000002-01"
        );
    }
}
