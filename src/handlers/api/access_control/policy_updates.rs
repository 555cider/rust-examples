use crate::{auth::AuthnUser, errors::AppError, services::policy::PolicyUpdateEvent, AppState};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Query parameters for policy update history
#[derive(Debug, Deserialize)]
pub struct PolicyUpdateHistoryQuery {
    pub limit: Option<usize>,
}

/// Response for policy update status
#[derive(Debug, Serialize)]
pub struct PolicyUpdateStatusResponse {
    pub listener_count: usize,
    pub subscriber_count: usize,
    pub total_updates: usize,
}

/// Request to trigger a policy update notification
#[derive(Debug, Deserialize)]
pub struct TriggerPolicyUpdateRequest {
    pub policy_id: String,
    pub policy_name: String,
    pub update_type: String, // "created", "updated", "deleted", "activated", "deactivated"
}

/// Request to trigger cache invalidation
#[derive(Debug, Deserialize)]
pub struct TriggerCacheInvalidationRequest {
    pub policy_ids: Vec<String>,
}

/// Get policy update status
pub async fn get_update_status(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
) -> Result<Json<PolicyUpdateStatusResponse>, AppError> {
    let update_manager = &app_state.policy_update_manager;
    let listener_count = update_manager.get_listener_count().await;
    let subscriber_count = update_manager.get_subscriber_count();
    let history = update_manager.get_update_history().await;
    let total_updates = history.len();

    let response = PolicyUpdateStatusResponse {
        listener_count,
        subscriber_count,
        total_updates,
    };

    Ok(Json(response))
}

/// Get policy update history
pub async fn get_update_history(
    State(app_state): State<Arc<AppState>>,
    Query(query): Query<PolicyUpdateHistoryQuery>,
    _auth_user: AuthnUser,
) -> Result<Json<Vec<PolicyUpdateEvent>>, AppError> {
    let update_manager = &app_state.policy_update_manager;
    let history = if let Some(limit) = query.limit {
        update_manager.get_recent_updates(limit).await
    } else {
        update_manager.get_update_history().await
    };

    Ok(Json(history))
}

/// Clear policy update history
pub async fn clear_update_history(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
) -> Result<StatusCode, AppError> {
    let update_manager = &app_state.policy_update_manager;
    update_manager
        .clear_history()
        .await
        .map_err(AppError::InternalServerError)?;

    Ok(StatusCode::OK)
}

/// Trigger a policy update notification (for testing/manual triggering)
pub async fn trigger_policy_update(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
    Json(request): Json<TriggerPolicyUpdateRequest>,
) -> Result<StatusCode, AppError> {
    let update_manager = &app_state.policy_update_manager;
    let result = match request.update_type.as_str() {
        "created" => {
            update_manager
                .notify_policy_created(request.policy_id, request.policy_name)
                .await
        }
        "updated" => {
            update_manager
                .notify_policy_updated(request.policy_id, request.policy_name)
                .await
        }
        "deleted" => {
            update_manager
                .notify_policy_deleted(request.policy_id, request.policy_name)
                .await
        }
        "activated" => {
            update_manager
                .notify_policy_activated(request.policy_id, request.policy_name)
                .await
        }
        "deactivated" => {
            update_manager
                .notify_policy_deactivated(request.policy_id, request.policy_name)
                .await
        }
        _ => {
            return Err(AppError::BadRequest(format!(
                "Invalid update type: {}",
                request.update_type
            )));
        }
    };

    result.map_err(|e| AppError::InternalServerError(e))?;
    Ok(StatusCode::OK)
}

/// Trigger cache invalidation
pub async fn trigger_cache_invalidation(
    State(app_state): State<Arc<AppState>>,
    _auth_user: AuthnUser,
    Json(request): Json<TriggerCacheInvalidationRequest>,
) -> Result<StatusCode, AppError> {
    let update_manager = &app_state.policy_update_manager;
    update_manager
        .notify_cache_invalidation(request.policy_ids)
        .await
        .map_err(|e| AppError::InternalServerError(e))?;

    Ok(StatusCode::OK)
}

use axum::{
    routing::{delete, get, post},
    Router,
};

/// Routes for policy update management
pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status", get(get_update_status))
        .route("/history", get(get_update_history))
        .route("/history", delete(clear_update_history))
        .route("/trigger", post(trigger_policy_update))
        .route("/invalidate-cache", post(trigger_cache_invalidation))
}
