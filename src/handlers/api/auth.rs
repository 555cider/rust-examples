use crate::{
    auth::AuthnUser,
    errors::AppError,
    models::dto::auth::{CurrentUserResponse, LoginRequest, LoginResponse, RegisterRequest},
    utils::cookies,
    AppState,
};
use axum::{
    extract::{ConnectInfo, Form, Request, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use axum_extra::{headers::UserAgent, TypedHeader};
use http::{header, header::HeaderValue, HeaderMap, StatusCode};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        // Public auth endpoints (no authentication required)
        .route("/login", post(post_auth_login))
        .route("/refresh", post(post_auth_refresh))
        .route("/register", post(post_auth_register))
        // Protected auth endpoints (authentication required)
        .merge(
            Router::new()
                .route("/me", get(get_auth_me))
                .route("/logout", post(post_auth_logout))
                .layer(axum::middleware::from_fn(crate::middleware::auth_user))
                .layer(axum::middleware::from_fn(crate::middleware::auth)),
        )
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "auth"
)]
async fn post_auth_login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    TypedHeader(user_agent): TypedHeader<UserAgent>,
    Form(mut req): Form<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("Login request for user: {}", req.username);

    if req.redirect_url.is_none() {
        req.redirect_url = Some("/dashboard".to_string());
    }

    let ip_address = Some(addr.ip().to_string());
    let user_agent = Some(user_agent.to_string());

    let (access_token, refresh_token) = state
        .service
        .auth_service
        .login(&state.config, req, ip_address, user_agent)
        .await?;

    let access_cookie = cookies::create_access_token_cookie(&state.config, &access_token);
    let refresh_cookie = cookies::create_refresh_token_cookie(&state.config, &refresh_token);

    let response = LoginResponse {
        access_token: access_token.clone(),
        token_type: "Bearer".to_string(),
        expires_in: state.config.token.access_exp,
        redirect_url: Some("/dashboard".to_string()),
    };

    // Set-Cookie 헤더를 명확하게 여러 개 추가
    let access_cookie_header = HeaderValue::from_str(&access_cookie.to_string()).map_err(|e| {
        AppError::InternalServerError(format!("Failed to create access cookie header: {}", e))
    })?;

    let refresh_cookie_header =
        HeaderValue::from_str(&refresh_cookie.to_string()).map_err(|e| {
            AppError::InternalServerError(format!("Failed to create refresh cookie header: {}", e))
        })?;

    let response_body = serde_json::to_string(&response).map_err(|e| {
        AppError::InternalServerError(format!("Failed to serialize response: {}", e))
    })?;

    let res = Response::builder()
        .status(StatusCode::OK)
        .header(header::SET_COOKIE, access_cookie_header)
        .header(header::SET_COOKIE, refresh_cookie_header)
        .header("Cache-Control", "no-store")
        .header("Pragma", "no-cache")
        .header("Content-Type", "application/json")
        .body(response_body)
        .map_err(|e| AppError::InternalServerError(format!("Failed to build response: {}", e)))?;

    Ok(res)
}

#[utoipa::path(
    post,
    path = "/api/auth/refresh",
    responses(
        (status = 200, description = "Token refresh successful", body = LoginResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "auth"
)]
async fn post_auth_refresh(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    TypedHeader(user_agent): TypedHeader<UserAgent>,
    request: Request,
) -> Result<impl IntoResponse, AppError> {
    let headers = request.headers();
    let refresh_token = cookies::get_refresh_token(Some(&state.config), headers)
        .ok_or_else(|| AppError::Unauthorized("No refresh token provided".to_string()))?;

    let ip_address = Some(addr.ip().to_string());
    let user_agent = Some(user_agent.to_string());

    let (access_token, new_refresh_token) = state
        .service
        .auth_service
        .refresh_access_token(
            &state.config,
            refresh_token.to_string(),
            ip_address,
            user_agent,
        )
        .await?;

    let response = LoginResponse {
        access_token: access_token.clone(),
        token_type: "Bearer".to_string(),
        expires_in: state.config.token.access_exp,
        redirect_url: None,
    };

    let access_cookie = cookies::create_access_token_cookie(&state.config, &access_token);
    let refresh_cookie = cookies::create_refresh_token_cookie(&state.config, &new_refresh_token);

    let mut headers = HeaderMap::new();

    let access_cookie_value = access_cookie.to_string().parse().map_err(|e| {
        AppError::InternalServerError(format!("Failed to parse access cookie: {}", e))
    })?;
    headers.insert(header::SET_COOKIE, access_cookie_value);

    let refresh_cookie_value = refresh_cookie.to_string().parse().map_err(|e| {
        AppError::InternalServerError(format!("Failed to parse refresh cookie: {}", e))
    })?;
    headers.append(header::SET_COOKIE, refresh_cookie_value);

    Ok((StatusCode::OK, headers, Json(response)))
}

#[utoipa::path(
    get,
    path = "/api/auth/me",
    responses(
        (status = 200, description = "Get current user info", body = CurrentUserResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "auth",
    security(
        ("bearer_auth" = [])
    )
)]
async fn get_auth_me(
    State(state): State<Arc<AppState>>,
    current_user: AuthnUser,
) -> Result<Json<CurrentUserResponse>, AppError> {
    let response = state
        .service
        .auth_service
        .get_current_user(current_user)
        .await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Registration successful", body = i64),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "auth"
)]
async fn post_auth_register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    TypedHeader(user_agent): TypedHeader<UserAgent>,
    Form(req): Form<RegisterRequest>,
) -> Result<(StatusCode, Json<i64>), AppError> {
    info!("Register request for username: {}", req.username);

    let ip_address = Some(addr.ip().to_string());
    let user_agent = Some(user_agent.to_string());

    let user_id = state
        .service
        .auth_service
        .register(req, ip_address, user_agent)
        .await?;

    Ok((StatusCode::CREATED, Json(user_id)))
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    responses(
        (status = 200, description = "Logout successful"),
        (status = 500, description = "Internal server error")
    ),
    tag = "auth",
    security(
        ("bearer_auth" = [])
    )
)]
async fn post_auth_logout(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    // 토큰 쿠키 삭제
    let access_cookie = cookies::expire_access_token_cookie(&state.config);
    let refresh_cookie = cookies::expire_refresh_token_cookie(&state.config);

    // 쿠키 헤더 추가
    let mut headers = HeaderMap::new();
    if let Ok(access_cookie_str) = access_cookie.to_string().parse::<HeaderValue>() {
        headers.append(header::SET_COOKIE, access_cookie_str);
    }
    if let Ok(refresh_cookie_str) = refresh_cookie.to_string().parse::<HeaderValue>() {
        headers.append(header::SET_COOKIE, refresh_cookie_str);
    }

    // 클라이언트 측에서도 토큰을 제거할 수 있도록 리다이렉트 응답 반환
    Ok((
        headers,
        Json(serde_json::json!({
            "success": true,
            "redirect": "/auth/login"
        })),
    ))
}
