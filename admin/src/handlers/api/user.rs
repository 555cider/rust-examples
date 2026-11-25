use crate::{
    errors::AppError,
    models::dto::{
        common::{IdQuery, ListQueryParams},
        user::{CreateUserRequest, UpdateUserRequest},
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
        .route("/", get(list_users).post(create_user))
        .route("/detail", get(get_user_detail))
        .route("/update", put(update_user))
        .route("/delete", delete(delete_user))
        .layer(axum::middleware::from_fn(crate::middleware::auth))
}

async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    let response = state.service.user_service.create_user(req).await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

async fn list_users(
    State(state): State<Arc<AppState>>,
    Query(list_query): Query<ListQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    let response = state
        .service
        .user_service
        .get_user_array(list_query)
        .await?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn get_user_detail(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> Result<impl IntoResponse, AppError> {
    let response = state.service.user_service.get_user_by_id(query.id).await?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn update_user(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    req.validate()?;
    state.service.user_service.update_user(query.id, req).await?;

    // Return updated user for consistency
    let updated_user = state.service.user_service.get_user_by_id(query.id).await?;
    Ok((StatusCode::OK, Json(updated_user)).into_response())
}

async fn delete_user(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> Result<impl IntoResponse, AppError> {
    state.service.user_service.delete_user(query.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
