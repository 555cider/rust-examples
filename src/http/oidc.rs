use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

use crate::state::AppState;

pub fn build_router() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(openid_configuration),
        )
        .route("/jwks.json", get(jwks))
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
