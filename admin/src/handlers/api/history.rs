use crate::{
    errors::AppError,
    models::dto::history::HistoryListQuery,
    AppState,
};
use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize)]
struct ListResponse<T> {
    items: Vec<T>,
    total: i64,
    page: i64,
    limit: i64,
}

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_history))
        .route("/{id}", get(get_history_by_id))
        .layer(axum::middleware::from_fn(crate::middleware::auth))
}

/// List history with pagination and filtering
async fn list_history(
    State(state): State<Arc<AppState>>,
    Query(mut query): Query<HistoryListQuery>,
) -> Result<impl IntoResponse, AppError> {
    // Set pagination defaults
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    // Update query with calculated values
    query.limit = Some(limit);
    query.offset = Some(offset);
    query.page = Some(page);

    // Get history items
    let items = state
        .service
        .history_service
        .get_recent_history(query.clone())
        .await?;

    // Get total count
    let mut count_query = query.clone();
    count_query.limit = None;
    count_query.offset = None;

    let total = state
        .service
        .history_service
        .count_history(&count_query)
        .await?;

    let response = ListResponse {
        items,
        total,
        page,
        limit,
    };

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Get a single history entry by ID
async fn get_history_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let history = state
        .service
        .history_service
        .get_history_by_id(id)
        .await?
        .ok_or_else(|| AppError::NotFound("History entry not found".to_string()))?;

    Ok((StatusCode::OK, Json(history)).into_response())
}
