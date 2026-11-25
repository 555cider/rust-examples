use crate::config::AppState;
use crate::errors::AppError;
use crate::middleware;
use crate::models::entities::access_evaluation_log::AccessEvaluationLog;
use crate::services::access_evaluation_log::StatsResponse;
use axum::{
    extract::{Path, Query, State},
    middleware as axum_middleware,
    response::Json,
    routing::get,
    Extension,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct ListLogsQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    pub search: Option<String>,
    pub result: Option<String>,
    pub evaluation_type: Option<String>,
    pub date_range: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    20
}

#[derive(Debug, Serialize)]
pub struct ListLogsResponse {
    pub items: Vec<AccessEvaluationLog>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_logs))
        .route("/stats", get(get_stats))
        .route("/{id}", get(get_log))
        .layer(axum_middleware::from_fn(middleware::auth))
}

/// List access logs with pagination and filtering
async fn list_logs(
    State(state): State<Arc<AppState>>,
    Extension(_user_id): Extension<middleware::UserId>,
    Query(query): Query<ListLogsQuery>,
) -> Result<Json<ListLogsResponse>, AppError> {
    let service = &state.service.access_evaluation_log_service;
    
    let offset = (query.page - 1) * query.limit;
    
    // Get logs with filters
    let logs = service
        .get_logs_with_filters(
            query.limit,
            offset,
            query.search.as_deref(),
            query.result.as_deref(),
            query.evaluation_type.as_deref(),
            query.date_range.as_deref(),
        )
        .await?;
    
    let total = service
        .count_logs_with_filters(
            query.search.as_deref(),
            query.result.as_deref(),
            query.evaluation_type.as_deref(),
            query.date_range.as_deref(),
        )
        .await?;
    
    Ok(Json(ListLogsResponse {
        items: logs,
        total,
        page: query.page,
        limit: query.limit,
    }))
}

/// Get a single log by ID
async fn get_log(
    State(state): State<Arc<AppState>>,
    Extension(_user_id): Extension<middleware::UserId>,
    Path(id): Path<i64>,
) -> Result<Json<AccessEvaluationLog>, AppError> {
    let service = &state.service.access_evaluation_log_service;
    let log = service.get_log_by_id(id).await?
        .ok_or_else(|| AppError::NotFound(format!("Log with id {} not found", id)))?;
    Ok(Json(log))
}

/// Get statistics
async fn get_stats(
    State(state): State<Arc<AppState>>,
    Extension(_user_id): Extension<middleware::UserId>,
    Query(query): Query<ListLogsQuery>,
) -> Result<Json<StatsResponse>, AppError> {
    let service = &state.service.access_evaluation_log_service;
    
    let stats = service
        .get_stats(
            query.search.as_deref(),
            query.result.as_deref(),
            query.evaluation_type.as_deref(),
            query.date_range.as_deref(),
        )
        .await?;
    
    Ok(Json(stats))
}
