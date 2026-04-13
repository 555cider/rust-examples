use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use serde::Serialize;
use sqlx::types::Uuid;

use crate::{identity::repository, state::AppState};

pub fn build_router() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(openid_configuration),
        )
        .route("/jwks.json", get(jwks))
        .route("/userinfo", get(userinfo))
}

async fn openid_configuration(State(state): State<AppState>) -> Json<OpenIdConfiguration> {
    let issuer = state.issuer.trim_end_matches('/').to_owned();

    Json(OpenIdConfiguration {
        issuer: issuer.clone(),
        jwks_uri: format!("{issuer}/jwks.json"),
        authorization_endpoint: format!("{issuer}/authorize"),
        token_endpoint: format!("{issuer}/token"),
        userinfo_endpoint: format!("{issuer}/userinfo"),
        response_types_supported: vec!["code"],
        subject_types_supported: vec!["public"],
        id_token_signing_alg_values_supported: vec!["EdDSA"],
        grant_types_supported: vec!["authorization_code", "client_credentials", "refresh_token"],
        token_endpoint_auth_methods_supported: vec![
            "client_secret_basic",
            "client_secret_post",
            "none",
        ],
        scopes_supported: vec!["openid", "profile", "email", "offline_access"],
        claims_supported: vec![
            "iss",
            "sub",
            "aud",
            "exp",
            "iat",
            "jti",
            "scope",
            "roles",
            "client_id",
            "azp",
        ],
    })
}

async fn jwks(State(state): State<AppState>) -> Json<crate::token::JwkSet> {
    Json(state.token_service.jwks())
}

async fn userinfo(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let authorization = match headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        Some(value) => value,
        None => return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response(),
    };
    let token = match authorization.strip_prefix("Bearer ") {
        Some(token) => token,
        None => return (StatusCode::UNAUTHORIZED, "invalid bearer token").into_response(),
    };

    let claims = match state.token_service.verify(token) {
        Ok(claims) => claims,
        Err(_) => return (StatusCode::UNAUTHORIZED, "invalid token").into_response(),
    };
    if !claims
        .scope
        .split_whitespace()
        .any(|scope| scope == "openid")
    {
        return (StatusCode::FORBIDDEN, "openid scope required").into_response();
    }

    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(user_id) => user_id,
        Err(_) => return (StatusCode::UNAUTHORIZED, "invalid subject").into_response(),
    };
    let profile = match repository::find_user_profile(&state.db_pool, user_id).await {
        Ok(Some(profile)) => profile,
        Ok(None) => return (StatusCode::UNAUTHORIZED, "unknown subject").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "server error").into_response(),
    };

    Json(UserInfoResponse {
        sub: profile.id.to_string(),
        preferred_username: profile.username,
        email: profile.email,
    })
    .into_response()
}

#[derive(Serialize)]
struct OpenIdConfiguration {
    issuer: String,
    jwks_uri: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    response_types_supported: Vec<&'static str>,
    subject_types_supported: Vec<&'static str>,
    id_token_signing_alg_values_supported: Vec<&'static str>,
    grant_types_supported: Vec<&'static str>,
    token_endpoint_auth_methods_supported: Vec<&'static str>,
    scopes_supported: Vec<&'static str>,
    claims_supported: Vec<&'static str>,
}

#[derive(Serialize)]
struct UserInfoResponse {
    sub: String,
    preferred_username: String,
    email: String,
}
