use crate::{
    auth::AuthnUser,
    config::access_control::{AccessControlMode, CombinationStrategy, ConflictResolution},
    errors::AppError,
    AppState,
};
use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Response for access control configuration
#[derive(Debug, Serialize)]
pub struct AccessControlConfigResponse {
    pub mode: String,
    pub combination_strategy: String,
    pub conflict_resolution: String,
    pub enable_caching: bool,
    pub cache_ttl_seconds: u64,
    pub max_evaluation_time_ms: u64,
    pub enable_audit_logging: bool,
}

/// Request to update access control mode
#[derive(Debug, Deserialize)]
pub struct UpdateModeRequest {
    pub mode: String,
}

/// Request to update combination strategy
#[derive(Debug, Deserialize)]
pub struct UpdateCombinationStrategyRequest {
    pub strategy: String,
}

/// Request to update conflict resolution
#[derive(Debug, Deserialize)]
pub struct UpdateConflictResolutionRequest {
    pub resolution: String,
}

/// Request to update caching settings
#[derive(Debug, Deserialize)]
pub struct UpdateCachingRequest {
    pub enabled: bool,
    pub ttl_seconds: Option<u64>,
}

/// Request to update evaluation timeout
#[derive(Debug, Deserialize)]
pub struct UpdateEvaluationTimeoutRequest {
    pub max_time_ms: u64,
}

/// Request to update audit logging
#[derive(Debug, Deserialize)]
pub struct UpdateAuditLoggingRequest {
    pub enabled: bool,
}

/// Get current access control configuration
pub async fn get_config(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
) -> Result<Json<AccessControlConfigResponse>, AppError> {
    let config_manager = &app_state.access_control_config;
    let config = config_manager.get_config();

    let response = AccessControlConfigResponse {
        mode: config.mode.to_string(),
        combination_strategy: format!("{:?}", config.combination_strategy),
        conflict_resolution: format!("{:?}", config.conflict_resolution),
        enable_caching: config.enable_caching,
        cache_ttl_seconds: config.cache_ttl_seconds,
        max_evaluation_time_ms: config.max_evaluation_time_ms,
        enable_audit_logging: config.enable_audit_logging,
    };

    Ok(Json(response))
}

/// Update access control mode
pub async fn update_mode(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
    Json(request): Json<UpdateModeRequest>,
) -> Result<StatusCode, AppError> {
    let config_manager = &app_state.access_control_config;
    let mode: AccessControlMode = request
        .mode
        .parse()
        .map_err(|e| AppError::BadRequest(format!("Invalid access control mode: {}", e)))?;

    config_manager
        .set_mode(mode)
        .map_err(AppError::InternalServerError)?;

    Ok(StatusCode::OK)
}

/// Update combination strategy
pub async fn update_combination_strategy(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
    Json(request): Json<UpdateCombinationStrategyRequest>,
) -> Result<StatusCode, AppError> {
    let config_manager = &app_state.access_control_config;
    let strategy: CombinationStrategy = request
        .strategy
        .parse()
        .map_err(|e| AppError::BadRequest(format!("Invalid combination strategy: {}", e)))?;

    config_manager
        .set_combination_strategy(strategy)
        .map_err(AppError::InternalServerError)?;

    Ok(StatusCode::OK)
}

/// Update conflict resolution
pub async fn update_conflict_resolution(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
    Json(request): Json<UpdateConflictResolutionRequest>,
) -> Result<StatusCode, AppError> {
    let config_manager = &app_state.access_control_config;
    let resolution: ConflictResolution = request
        .resolution
        .parse()
        .map_err(|e| AppError::BadRequest(format!("Invalid conflict resolution: {}", e)))?;

    config_manager
        .set_conflict_resolution(resolution)
        .map_err(AppError::InternalServerError)?;

    Ok(StatusCode::OK)
}

/// Update caching settings
pub async fn update_caching(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
    Json(request): Json<UpdateCachingRequest>,
) -> Result<StatusCode, AppError> {
    let config_manager = &app_state.access_control_config;
    config_manager
        .set_caching_enabled(request.enabled)
        .map_err(AppError::InternalServerError)?;

    if let Some(ttl) = request.ttl_seconds {
        config_manager
            .set_cache_ttl(ttl)
            .map_err(AppError::InternalServerError)?;
    }

    Ok(StatusCode::OK)
}

/// Update evaluation timeout
pub async fn update_evaluation_timeout(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
    Json(request): Json<UpdateEvaluationTimeoutRequest>,
) -> Result<StatusCode, AppError> {
    let config_manager = &app_state.access_control_config;
    config_manager
        .set_max_evaluation_time(request.max_time_ms)
        .map_err(AppError::InternalServerError)?;

    Ok(StatusCode::OK)
}

/// Update audit logging
pub async fn update_audit_logging(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
    Json(request): Json<UpdateAuditLoggingRequest>,
) -> Result<StatusCode, AppError> {
    let config_manager = &app_state.access_control_config;
    config_manager
        .set_audit_logging_enabled(request.enabled)
        .map_err(AppError::InternalServerError)?;

    Ok(StatusCode::OK)
}

use axum::{
    routing::{get, put},
    Router,
};
/// Routes for access control configuration management
pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_config))
        .route("/mode", put(update_mode))
        .route("/combination-strategy", put(update_combination_strategy))
        .route("/conflict-resolution", put(update_conflict_resolution))
        .route("/caching", put(update_caching))
        .route("/evaluation-timeout", put(update_evaluation_timeout))
        .route("/audit-logging", put(update_audit_logging))
}
