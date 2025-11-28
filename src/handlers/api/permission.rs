use crate::{
    errors::AppError,
    models::dto::{
        common::{IdQuery, ListQueryParams, ListResponse},
        permission::{CreatePermissionRequest, UpdatePermissionRequest},
    },
    AppState,
};
use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, put},
    Router,
};
use std::sync::Arc;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_permissions).post(create_permission))
        .route("/detail", get(get_permission_detail))
        .route("/update", put(update_permission))
        .route("/delete", delete(delete_permission))
        .layer(axum::middleware::from_fn(crate::middleware::auth))
}

async fn create_permission(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreatePermissionRequest>,
) -> Result<impl IntoResponse, AppError> {
    let response = state
        .service
        .permission_service
        .create_permission(req)
        .await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

async fn list_permissions(
    State(state): State<Arc<AppState>>,
    Query(list_query): Query<ListQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    let items = state
        .service
        .permission_service
        .get_permissions(list_query.clone())
        .await?;

    let total = state
        .service
        .permission_service
        .count_permissions()
        .await?;

    let response = ListResponse::new(
        items,
        total,
        list_query.page.unwrap_or(1),
        list_query.get_limit(),
    );

    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn get_permission_detail(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> Result<impl IntoResponse, AppError> {
    let response = state
        .service
        .permission_service
        .get_permission_by_id(query.id)
        .await?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn update_permission(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
    Json(req): Json<UpdatePermissionRequest>,
) -> Result<impl IntoResponse, AppError> {
    let response = state
        .service
        .permission_service
        .update_permission(query.id, req)
        .await?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn delete_permission(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> Result<StatusCode, AppError> {
    state
        .service
        .permission_service
        .delete_permission(query.id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
