use crate::config::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tera::Context;
use tracing::{debug, error};

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/login", get(login_page).post(login_handler))
        .route("/register", get(register_page))
        .route("/logout", post(logout_handler))
}

async fn login_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let mut context = Context::new();
    context.insert("title", "로그인");
    context.insert("active_page", "login");

    if let Some(error_message) = query.get("error") {
        context.insert("error", error_message);
    }

    match state.tera.render("pages/auth/login.html", &context) {
        Ok(s) => Html(s).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Template rendering error: {}", e),
            )
                .into_response()
        }
    }
}

async fn login_handler(
    State(state): State<Arc<AppState>>,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    debug!("Login handler called for user: {}", form.username);

    // Convert form to LoginRequest
    let login_request = crate::models::dto::auth::LoginRequest {
        username: form.username.clone(),
        password: form.password.clone(),
        redirect_url: Some("/dashboard".to_string()),
    };

    // Attempt authentication
    match state
        .service
        .auth_service
        .login(&state.config, login_request, None, None)
        .await
    {
        Ok((access_token, refresh_token)) => {
            // Create cookies
            let access_cookie =
                crate::utils::cookies::create_access_token_cookie(&state.config, &access_token);
            let refresh_cookie =
                crate::utils::cookies::create_refresh_token_cookie(&state.config, &refresh_token);

            // Create response with cookies and redirect
            let mut response = Redirect::to("/dashboard").into_response();

            // Add cookies to response
            if let Ok(access_cookie_str) = access_cookie.to_string().parse() {
                response
                    .headers_mut()
                    .append(axum::http::header::SET_COOKIE, access_cookie_str);
            }
            if let Ok(refresh_cookie_str) = refresh_cookie.to_string().parse() {
                response
                    .headers_mut()
                    .append(axum::http::header::SET_COOKIE, refresh_cookie_str);
            }

            response
        }
        Err(e) => {
            // Redirect back to login with error
            let error_message = match e {
                crate::errors::AppError::Unauthorized(_) => {
                    "아이디 또는 비밀번호가 올바르지 않습니다."
                }
                _ => "로그인 중 오류가 발생했습니다.",
            };

            Redirect::to(&format!(
                "/auth/login?error={}",
                urlencoding::encode(error_message)
            ))
            .into_response()
        }
    }
}

async fn register_page(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut context = Context::new();
    context.insert("title", "회원가입");
    context.insert("active_page", "register");

    match state.tera.render("pages/auth/register.html", &context) {
        Ok(s) => Html(s).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Template rendering error",
            )
                .into_response()
        }
    }
}

async fn logout_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    debug!("Logout handler called");

    // Create expired cookies to clear them
    let access_cookie = crate::utils::cookies::expire_access_token_cookie(&state.config);
    let refresh_cookie = crate::utils::cookies::expire_refresh_token_cookie(&state.config);

    // Create response with cookies and redirect
    let mut response = Redirect::to("/auth/login").into_response();

    // Add expired cookies to response
    if let Ok(access_cookie_str) = access_cookie.to_string().parse() {
        response
            .headers_mut()
            .append(axum::http::header::SET_COOKIE, access_cookie_str);
    }
    if let Ok(refresh_cookie_str) = refresh_cookie.to_string().parse() {
        response
            .headers_mut()
            .append(axum::http::header::SET_COOKIE, refresh_cookie_str);
    }

    response
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
    #[allow(dead_code)] // Used for form compatibility but not processed yet
    remember: Option<String>,
}
