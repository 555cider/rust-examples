use crate::{
    errors::AppError,
    models::dto::{
        common::{IdQuery, ListQueryParams, ListResponse},
        role::{CreateRoleRequest, UpdateRoleRequest},
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
use validator::Validate;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_roles).post(create_role))
        .route("/detail", get(get_role_detail))
        .route("/update", put(update_role))
        .route("/delete", delete(delete_role))
        .layer(axum::middleware::from_fn(crate::middleware::auth))
}

async fn create_role(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let response = state.service.role_service.create_role(req).await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

async fn list_roles(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    let items = state
        .service
        .role_service
        .get_roles(query.clone())
        .await?;

    let total = state.service.role_service.count_roles(&query).await?;

    let response = ListResponse::new(
        items,
        total,
        query.page.unwrap_or(1),
        query.get_limit(),
    );

    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn get_role_detail(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> Result<impl IntoResponse, AppError> {
    let response = state.service.role_service.get_role_by_id(query.id).await?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn update_role(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
    Json(req): Json<UpdateRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    req.validate()?;
    let response = state
        .service
        .role_service
        .update_role(query.id, req)
        .await?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn delete_role(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> Result<impl IntoResponse, AppError> {
    state.service.role_service.delete_role(query.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
