use crate::entity;
use hyper::StatusCode;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug, Clone)]
pub struct MetricsCollector(Arc<Mutex<MetricsState>>);

#[derive(Debug, Default)]
struct MetricsState {
    requests_total: u64,
    routes: BTreeMap<String, RouteMetricsState>,
    errors_by_kind: BTreeMap<String, u64>,
}

#[derive(Debug, Default)]
struct RouteMetricsState {
    requests: u64,
    responses_by_status: BTreeMap<String, u64>,
    total_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub requests_total: u64,
    pub routes: Vec<RouteMetricsSnapshot>,
    pub errors_by_kind: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteMetricsSnapshot {
    pub route: String,
    pub requests: u64,
    pub responses_by_status: BTreeMap<String, u64>,
    pub total_duration_ms: u64,
}

pub fn new_collector() -> MetricsCollector {
    MetricsCollector(Arc::new(Mutex::new(MetricsState::default())))
}

pub fn record(
    collector: &MetricsCollector,
    route: &str,
    status: StatusCode,
    duration: Duration,
    error: Option<&entity::GatewayError>,
) {
    let mut state = collector.0.lock().unwrap();
    state.requests_total = state.requests_total.saturating_add(1);

    let route_state = state.routes.entry(route.to_string()).or_default();
    route_state.requests = route_state.requests.saturating_add(1);
    let status_key = status.as_u16().to_string();
    let status_count = route_state
        .responses_by_status
        .entry(status_key)
        .or_insert(0);
    *status_count = status_count.saturating_add(1);
    route_state.total_duration_ms = route_state
        .total_duration_ms
        .saturating_add(duration.as_millis().min(u128::from(u64::MAX)) as u64);

    if let Some(error) = error {
        let error_count = state.errors_by_kind.entry(error_kind(error)).or_insert(0);
        *error_count = error_count.saturating_add(1);
    }
}

pub fn snapshot(collector: &MetricsCollector) -> MetricsSnapshot {
    let state = collector.0.lock().unwrap();

    MetricsSnapshot {
        requests_total: state.requests_total,
        routes: state
            .routes
            .iter()
            .map(|(route, metrics)| RouteMetricsSnapshot {
                route: route.clone(),
                requests: metrics.requests,
                responses_by_status: metrics.responses_by_status.clone(),
                total_duration_ms: metrics.total_duration_ms,
            })
            .collect(),
        errors_by_kind: state.errors_by_kind.clone(),
    }
}

fn error_kind(error: &entity::GatewayError) -> String {
    match error {
        entity::GatewayError::Hyper(_) => "Hyper",
        entity::GatewayError::Io(_) => "Io",
        entity::GatewayError::Http(_) => "Http",
        entity::GatewayError::Timeout(_) => "Timeout",
        entity::GatewayError::Discovery(_) => "Discovery",
        entity::GatewayError::RouteNotFound(_) => "RouteNotFound",
        entity::GatewayError::CircuitOpen(_) => "CircuitOpen",
        entity::GatewayError::InvalidRouteConfig(_) => "InvalidRouteConfig",
    }
    .to_string()
}
