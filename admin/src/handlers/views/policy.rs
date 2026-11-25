use crate::{middleware::UserId, AppState};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::sync::Arc;
use tera::Context;
use tracing::error;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_policies))
        .route("/create", get(create_policy_form))
        .route("/edit/{id}", get(edit_policy_form))
}

/// List policies page
async fn list_policies(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
) -> impl IntoResponse {
    let mut context = Context::new();
    context.insert("active_page", "policies");
    context.insert("user_id", &user_id.0);

    if let Ok(current_user) = state.service.user_service.get_user_by_id(user_id.0).await {
        context.insert("current_user", &current_user);
    }

    match state.tera.render("pages/policies/list.html", &context) {
        Ok(s) => Html(s).into_response(),
        Err(e) => {
            error!("Template error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
        }
    }
}

/// Create policy form page
async fn create_policy_form(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
) -> impl IntoResponse {
    let mut context = Context::new();
    context.insert("active_page", "policies");
    context.insert("user_id", &user_id.0);

    if let Ok(current_user) = state.service.user_service.get_user_by_id(user_id.0).await {
        context.insert("current_user", &current_user);
    }

    match state.tera.render("pages/policies/form.html", &context) {
        Ok(s) => Html(s).into_response(),
        Err(e) => {
            error!("Template error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
        }
    }
}

/// Edit policy form page
async fn edit_policy_form(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(user_id): Extension<UserId>,
) -> impl IntoResponse {
    let mut context = Context::new();
    context.insert("active_page", "policies");
    context.insert("user_id", &user_id.0);

    if let Ok(current_user) = state.service.user_service.get_user_by_id(user_id.0).await {
        context.insert("current_user", &current_user);
    }

    // Get policy from repository
    let policy_repo = &state.service.policy_repository;
    match policy_repo.find_by_id(&id).await {
        Ok(policy) => {
            context.insert("policy", &policy);
            match state.tera.render("pages/policies/form.html", &context) {
                Ok(s) => Html(s).into_response(),
                Err(e) => {
                    error!("Template error: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
                }
            }
        }
        Err(e) => {
            error!("Failed to get policy: {}", e);
            (StatusCode::NOT_FOUND, "Policy not found").into_response()
        }
    }
}
