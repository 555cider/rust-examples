use crate::{middleware::UserId, models::dto::common::ListQueryParams, AppState};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::{error::Error, sync::Arc};
use tera::Context;
use tracing::error;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(users_page))
        .route("/create", get(user_create_page))
        .route("/edit/{id}", get(user_edit_page))
}

async fn users_page(
    State(state): State<Arc<AppState>>,
    Extension(_user_id): Extension<UserId>,
) -> impl IntoResponse {
    let mut context = Context::new();
    context.insert("title", "사용자 관리");
    context.insert("active_page", "users");
    context.insert("user_id", &_user_id.0);

    // Add current user info for the template
    if let Ok(current_user) = state.service.user_service.get_user_by_id(_user_id.0).await {
        context.insert("current_user", &current_user);
    }

    match state.tera.render("pages/users/list.html", &context) {
        Ok(s) => Html(s).into_response(),
        Err(e) => {
            // Log the full error chain for better debugging
            let mut error_chain = String::new();
            let mut source = e.source();
            error_chain.push_str(&format!("Error: {}", e));

            while let Some(err) = source {
                error_chain.push_str(&format!("\nCaused by: {}", err));
                source = err.source();
            }

            error!(
                "Template rendering error: {}\nContext: {:#?}",
                error_chain, context
            );

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Template rendering error: {}", e),
            )
                .into_response()
        }
    }
}

async fn user_create_page(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
) -> impl IntoResponse {
    let mut context = Context::new();
    context.insert("title", "사용자 추가");
    context.insert("active_page", "users");
    context.insert("user_id", &user_id.0);

    // Add current user info for the template
    if let Ok(current_user) = state.service.user_service.get_user_by_id(user_id.0).await {
        context.insert("current_user", &current_user);
    }

    // Get user types for the form
    match state
        .service
        .role_service
        .get_roles(ListQueryParams::default())
        .await
    {
        Ok(roles) => {
            context.insert("roles", &roles);
        }
        Err(e) => {
            error!("Failed to load user types: {}", e);
            context.insert("error", "사용자 유형을 불러오는 중 오류가 발생했습니다.");
        }
    }

    match state.tera.render("pages/users/form.html", &context) {
        Ok(s) => Html(s).into_response(),
        Err(e) => {
            // Log the full error chain for better debugging
            let mut error_chain = String::new();
            let mut source = e.source();
            error_chain.push_str(&format!("Error: {}", e));

            while let Some(err) = source {
                error_chain.push_str(&format!("\nCaused by: {}", err));
                source = err.source();
            }

            error!(
                "Template rendering error: {}. Full error: {}\nContext: {:#?}",
                e.to_string(), error_chain, context
            );

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Template rendering error: {}", e),
            )
                .into_response()
        }
    }
}

async fn user_edit_page(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
    Path(edit_user_id): Path<i64>,
) -> impl IntoResponse {
    let mut context = Context::new();
    context.insert("title", "사용자 수정");
    context.insert("active_page", "users");
    context.insert("user_id", &user_id.0);

    // Add current user info for the template
    if let Ok(current_user) = state.service.user_service.get_user_by_id(user_id.0).await {
        context.insert("current_user", &current_user);
    }

    // Get user data
    match state
        .service
        .user_service
        .get_user_by_id(edit_user_id)
        .await
    {
        Ok(user_data) => {
            // Convert the user data to a format the template can use
            let user = serde_json::json!({
                "id": user_data.id,
                "username": user_data.username,
                "role_id": user_data.role_id,
                "is_active": user_data.is_active,
                "last_login_at": user_data.last_login_at,
                "created_at": user_data.created_at,
                "updated_at": user_data.updated_at
            });
            context.insert("user", &user);

            // Get user types for the form
            match state
                .service
                .role_service
                .get_roles(ListQueryParams::default())
                .await
            {
                Ok(roles) => {
                    context.insert("roles", &roles);
                }
                Err(e) => {
                    error!("Failed to load user types: {}", e);
                    context.insert("error", "사용자 유형을 불러오는 중 오류가 발생했습니다.");
                }
            }

            match state.tera.render("pages/users/form.html", &context) {
                Ok(s) => Html(s).into_response(),
                        Err(e) => {
                            // Log the full error chain for better debugging
                            let mut error_chain = String::new();
                            let mut source = e.source();
                            error_chain.push_str(&format!("Error: {}", e));
                
                            while let Some(err) = source {
                                error_chain.push_str(&format!("\nCaused by: {}", err));
                                source = err.source();
                            }
                
                            error!(
                                "Template rendering error: {}. Full error: {}\nContext: {:#?}",
                                e.to_string(), error_chain, context
                            );
                
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("Template rendering error: {}", e),
                            )
                                .into_response()
                        }            }
        }
        Err(e) => {
            error!("Failed to load user: {}", e);
            (StatusCode::NOT_FOUND, "User not found").into_response()
        }
    }
}
