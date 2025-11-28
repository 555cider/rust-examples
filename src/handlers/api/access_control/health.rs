use crate::{
    errors::AppError,
    services::access_control::{ErrorHandlingStatistics, HealthCheckSummary, SystemHealthStatus},
    AppState,
};
use axum::{
    extract::{Query, State},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Health check endpoint for access control system
pub async fn get_health_status(
    State(app_state): State<Arc<AppState>>,
) -> Result<Json<SystemHealthStatus>, AppError> {
    let health_status = app_state
        .access_control_error_handler
        .get_health_status()
        .await;
    Ok(Json(health_status))
}

/// Detailed health check endpoint
pub async fn perform_health_check(
    State(app_state): State<Arc<AppState>>,
) -> Result<Json<HealthCheckSummary>, AppError> {
    let health_check = app_state
        .access_control_error_handler
        .perform_health_check()
        .await;
    Ok(Json(health_check))
}

/// Get error handling statistics
pub async fn get_error_statistics(
    State(app_state): State<Arc<AppState>>,
) -> Result<Json<ErrorHandlingStatistics>, AppError> {
    let statistics = app_state
        .access_control_error_handler
        .get_error_statistics()
        .await;
    Ok(Json(statistics))
}

/// Force recovery of all components (admin operation)
pub async fn force_recovery(
    State(app_state): State<Arc<AppState>>,
) -> Result<Json<RecoveryResponse>, AppError> {
    match app_state
        .access_control_error_handler
        .force_recovery()
        .await
    {
        Ok(_) => Ok(Json(RecoveryResponse {
            success: true,
            message: "All access control components have been reset".to_string(),
            timestamp: chrono::Utc::now(),
        })),
        Err(e) => Err(AppError::InternalServerError(format!(
            "Failed to force recovery: {}",
            e
        ))),
    }
}

/// Get system metrics for monitoring
pub async fn get_system_metrics(
    State(app_state): State<Arc<AppState>>,
) -> Result<Json<SystemMetrics>, AppError> {
    let health_status = app_state
        .access_control_error_handler
        .get_health_status()
        .await;
    let _error_stats = app_state
        .access_control_error_handler
        .get_error_statistics()
        .await;

    let mut component_metrics = HashMap::new();

    for cb_stats in &health_status.circuit_breakers {
        let metrics = ComponentMetrics {
            name: cb_stats.component_name.clone(),
            state: format!("{:?}", cb_stats.state),
            failure_count: cb_stats.failure_count,
            success_count: cb_stats.success_count,
            is_healthy: cb_stats.state.to_string() == "closed",
        };
        component_metrics.insert(cb_stats.component_name.clone(), metrics);
    }

    let system_metrics = SystemMetrics {
        overall_health: health_status.overall_health,
        is_degraded: health_status.degradation.is_degraded,
        degradation_strategy: health_status.degradation.strategy.to_string(),
        component_metrics,
        timestamp: chrono::Utc::now(),
    };

    Ok(Json(system_metrics))
}

/// Configuration endpoint to update error handling settings
#[derive(Deserialize)]
pub struct UpdateConfigRequest {
    pub degradation_strategy: Option<String>,
    pub circuit_breaker_failure_threshold: Option<u32>,
    pub circuit_breaker_timeout_seconds: Option<u64>,
}

pub async fn update_error_handling_config(
    State(_app_state): State<Arc<AppState>>,
    Json(request): Json<UpdateConfigRequest>,
) -> Result<Json<ConfigUpdateResponse>, AppError> {
    // Validate the input and return a response
    // Configuration updates are applied through the error handler

    if let Some(strategy) = &request.degradation_strategy {
        match strategy.as_str() {
            "fail_fast" | "fallback_to_rbac" | "allow_on_failure" | "deny_on_failure" => {}
            _ => {
                return Err(AppError::BadRequest(format!(
                    "Invalid degradation strategy: {}",
                    strategy
                )))
            }
        }
    }

    if let Some(threshold) = request.circuit_breaker_failure_threshold {
        if threshold == 0 || threshold > 100 {
            return Err(AppError::BadRequest(
                "Circuit breaker failure threshold must be between 1 and 100".to_string(),
            ));
        }
    }

    if let Some(timeout) = request.circuit_breaker_timeout_seconds {
        if timeout == 0 || timeout > 3600 {
            return Err(AppError::BadRequest(
                "Circuit breaker timeout must be between 1 and 3600 seconds".to_string(),
            ));
        }
    }

    Ok(Json(ConfigUpdateResponse {
        success: true,
        message: "Configuration validation passed. Note: Dynamic configuration updates not yet implemented.".to_string(),
        applied_changes: vec![
            request.degradation_strategy.map(|s| format!("degradation_strategy: {}", s)),
            request.circuit_breaker_failure_threshold.map(|t| format!("failure_threshold: {}", t)),
            request.circuit_breaker_timeout_seconds.map(|t| format!("timeout_seconds: {}", t)),
        ].into_iter().flatten().collect(),
        timestamp: chrono::Utc::now(),
    }))
}

/// Response structures
#[derive(Serialize)]
pub struct RecoveryResponse {
    pub success: bool,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct SystemMetrics {
    pub overall_health: bool,
    pub is_degraded: bool,
    pub degradation_strategy: String,
    pub component_metrics: HashMap<String, ComponentMetrics>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct ComponentMetrics {
    pub name: String,
    pub state: String,
    pub failure_count: u32,
    pub success_count: u32,
    pub is_healthy: bool,
}

#[derive(Serialize)]
pub struct ConfigUpdateResponse {
    pub success: bool,
    pub message: String,
    pub applied_changes: Vec<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Query parameters for health check endpoints
#[derive(Deserialize)]
pub struct HealthCheckQuery {
    pub include_details: Option<bool>,
    pub component: Option<String>,
}

/// Filtered health check endpoint
pub async fn get_component_health(
    State(app_state): State<Arc<AppState>>,
    Query(query): Query<HealthCheckQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let health_status = app_state
        .access_control_error_handler
        .get_health_status()
        .await;

    if let Some(component_name) = query.component {
        // Return health for specific component
        let component_health = health_status
            .circuit_breakers
            .iter()
            .find(|cb| cb.component_name.eq_ignore_ascii_case(&component_name))
            .ok_or_else(|| {
                AppError::NotFound(format!("Component '{}' not found", component_name))
            })?;

        Ok(Json(serde_json::to_value(component_health).unwrap()))
    } else if query.include_details.unwrap_or(false) {
        // Return full health status
        Ok(Json(serde_json::to_value(health_status).unwrap()))
    } else {
        // Return summary only
        let summary = serde_json::json!({
            "overall_health": health_status.overall_health,
            "is_degraded": health_status.degradation.is_degraded,
            "component_count": health_status.circuit_breakers.len(),
            "healthy_components": health_status.circuit_breakers.iter()
                .filter(|cb| cb.state.to_string() == "closed")
                .count(),
            "timestamp": health_status.timestamp
        });

        Ok(Json(summary))
    }
}

// Note: Integration tests for these endpoints are in tests/integration_test.rs
// Unit tests require full AppState setup including database and template engine

/// Run circuit breaker demonstration (admin/testing endpoint)
pub async fn run_demonstration(
    State(_app_state): State<Arc<AppState>>,
) -> Result<Json<DemonstrationResponse>, AppError> {
    use crate::services::access_control::run_circuit_breaker_demonstration;

    match run_circuit_breaker_demonstration().await {
        Ok(_) => Ok(Json(DemonstrationResponse {
            success: true,
            message: "Circuit breaker demonstration completed successfully".to_string(),
            timestamp: chrono::Utc::now(),
        })),
        Err(e) => Err(AppError::InternalServerError(format!(
            "Demonstration failed: {}",
            e
        ))),
    }
}

#[derive(Serialize)]
pub struct DemonstrationResponse {
    pub success: bool,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Router configuration for access control health endpoints
pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status", get(get_health_status))
        .route("/check", get(perform_health_check))
        .route("/statistics", get(get_error_statistics))
        .route("/metrics", get(get_system_metrics))
        .route("/component", get(get_component_health))
        .route("/recovery", post(force_recovery))
        .route("/config", post(update_error_handling_config))
        .route("/demonstration", post(run_demonstration))
}
