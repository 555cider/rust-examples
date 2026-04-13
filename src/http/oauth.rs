use axum::{
    Form, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::{
    http::auth::SESSION_COOKIE_NAME,
    identity::session,
    oauth::{self, AuthorizationRequest, OAuthError, TokenRequest},
    state::AppState,
    token::{AccessTokenParams, IdTokenParams},
};

const ACCESS_TOKEN_TTL_SECONDS: u64 = 300;

#[derive(Deserialize)]
struct AuthorizeQuery {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    scope: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    nonce: Option<String>,
}

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: u64,
    scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
}

#[derive(Serialize)]
struct OAuthErrorResponse<'a> {
    error: &'a str,
}

pub fn build_router() -> Router<AppState> {
    Router::new()
        .route("/authorize", get(authorize))
        .route("/token", post(token))
}

async fn authorize(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<AuthorizeQuery>,
) -> impl IntoResponse {
    if query.response_type != "code" {
        return oauth_error_response(OAuthError::UnsupportedGrantType);
    }

    let session_cookie = match jar.get(SESSION_COOKIE_NAME) {
        Some(cookie) => cookie,
        None => return (StatusCode::UNAUTHORIZED, "missing session").into_response(),
    };
    let session_token_hash = session::hash_session_token(session_cookie.value());
    let Some(user_id) = oauth::find_active_session_user_id(&state.db_pool, &session_token_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        .unwrap_or(None)
    else {
        return (StatusCode::UNAUTHORIZED, "invalid session").into_response();
    };

    let Some(client) = oauth::find_oauth_client(&state.db_pool, &query.client_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        .unwrap_or(None)
    else {
        return oauth_error_response(OAuthError::InvalidClient);
    };

    let requested_scope = query.scope.as_deref().unwrap_or("");
    if let Err(error) = oauth::validate_authorization_request(
        &client,
        &AuthorizationRequest {
            client_id: &query.client_id,
            redirect_uri: &query.redirect_uri,
            scope: requested_scope,
            code_challenge: query.code_challenge.as_deref(),
            code_challenge_method: query.code_challenge_method.as_deref(),
        },
    ) {
        return oauth_error_response(error);
    }

    let code = match oauth::store_authorization_code(
        &state.db_pool,
        client.id,
        user_id,
        &query.redirect_uri,
        requested_scope,
        query.nonce.as_deref(),
        query.code_challenge.as_deref(),
        query.code_challenge_method.as_deref(),
    )
    .await
    {
        Ok(code) => code,
        Err(_) => return oauth_error_response(OAuthError::InvalidRequest("server_error")),
    };

    let mut redirect_uri: url::Url = match url::Url::parse(&query.redirect_uri) {
        Ok(url) => url,
        Err(_) => return oauth_error_response(OAuthError::InvalidRequest("redirect_uri")),
    };
    {
        let mut pairs = redirect_uri.query_pairs_mut();
        pairs.append_pair("code", &code.plain_text);
        if let Some(state_value) = &query.state {
            pairs.append_pair("state", state_value);
        }
    }

    Redirect::to(redirect_uri.as_str()).into_response()
}

async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<TokenRequest>,
) -> impl IntoResponse {
    let authorization_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let client_auth = match oauth::parse_client_authentication(
        authorization_header,
        form.client_id.as_deref(),
        form.client_secret.as_deref(),
    ) {
        Ok(auth) => auth,
        Err(error) => return oauth_error_response(error),
    };
    let client =
        match oauth::authenticate_client(&state.db_pool, client_auth, form.client_id.as_deref())
            .await
        {
            Ok(client) => client,
            Err(error) => return oauth_error_response(error),
        };

    match form.grant_type.as_str() {
        "authorization_code" => handle_authorization_code_token(state, client, form).await,
        "client_credentials" => handle_client_credentials_token(state, client, form).await,
        "refresh_token" => handle_refresh_token(state, client, form).await,
        _ => oauth_error_response(OAuthError::UnsupportedGrantType),
    }
}

async fn handle_authorization_code_token(
    state: AppState,
    client: oauth::OAuthClient,
    form: TokenRequest,
) -> axum::response::Response {
    let Some(code) = form.code.as_deref() else {
        return oauth_error_response(OAuthError::InvalidRequest("code"));
    };
    let Some(redirect_uri) = form.redirect_uri.as_deref() else {
        return oauth_error_response(OAuthError::InvalidRequest("redirect_uri"));
    };

    let authorization_code = match oauth::consume_authorization_code(
        &state.db_pool,
        &client,
        code,
        redirect_uri,
        form.code_verifier.as_deref(),
    )
    .await
    {
        Ok(record) => record,
        Err(error) => return oauth_error_response(error),
    };

    let refresh_token = if client
        .grant_types
        .iter()
        .any(|grant| grant == "refresh_token")
        && authorization_code
            .scope
            .split_whitespace()
            .any(|scope| scope == "offline_access")
    {
        match oauth::issue_refresh_token(
            &state.db_pool,
            authorization_code.client_id,
            Some(authorization_code.user_id),
            &authorization_code.scope,
            None,
        )
        .await
        {
            Ok(token) => Some(token.plain_text),
            Err(_) => return oauth_error_response(OAuthError::InvalidRequest("server_error")),
        }
    } else {
        None
    };
    let id_token = if authorization_code
        .scope
        .split_whitespace()
        .any(|scope| scope == "openid")
    {
        match state.token_service.issue_id_token(IdTokenParams {
            subject: &authorization_code.user_id.to_string(),
            audience: &client.client_id,
            nonce: authorization_code.nonce.as_deref(),
            ttl_seconds: ACCESS_TOKEN_TTL_SECONDS,
        }) {
            Ok(token) => Some(token),
            Err(_) => return oauth_error_response(OAuthError::InvalidRequest("server_error")),
        }
    } else {
        None
    };

    let access_token = match state.token_service.issue_access_token(AccessTokenParams {
        subject: &authorization_code.user_id.to_string(),
        audience: &client.client_id,
        scope: &authorization_code.scope,
        roles: None,
        client_id: Some(&client.client_id),
        authorized_party: Some(&client.client_id),
        ttl_seconds: ACCESS_TOKEN_TTL_SECONDS,
    }) {
        Ok(token) => token,
        Err(_) => return oauth_error_response(OAuthError::InvalidRequest("server_error")),
    };

    (
        StatusCode::OK,
        axum::Json(TokenResponse {
            access_token,
            token_type: "Bearer",
            expires_in: ACCESS_TOKEN_TTL_SECONDS,
            scope: authorization_code.scope,
            refresh_token,
            id_token,
        }),
    )
        .into_response()
}

async fn handle_client_credentials_token(
    state: AppState,
    client: oauth::OAuthClient,
    form: TokenRequest,
) -> axum::response::Response {
    if !client
        .grant_types
        .iter()
        .any(|grant| grant == "client_credentials")
    {
        return oauth_error_response(OAuthError::UnauthorizedClient);
    }

    let requested_scope = form.scope.as_deref().unwrap_or("");
    if !requested_scope.is_empty()
        && !requested_scope
            .split_whitespace()
            .all(|scope| client.scopes.iter().any(|allowed| allowed == scope))
    {
        return oauth_error_response(OAuthError::InvalidRequest("scope"));
    }

    let access_token = match state.token_service.issue_access_token(AccessTokenParams {
        subject: &client.client_id,
        audience: &client.client_id,
        scope: requested_scope,
        roles: None,
        client_id: Some(&client.client_id),
        authorized_party: Some(&client.client_id),
        ttl_seconds: ACCESS_TOKEN_TTL_SECONDS,
    }) {
        Ok(token) => token,
        Err(_) => return oauth_error_response(OAuthError::InvalidRequest("server_error")),
    };

    (
        StatusCode::OK,
        axum::Json(TokenResponse {
            access_token,
            token_type: "Bearer",
            expires_in: ACCESS_TOKEN_TTL_SECONDS,
            scope: requested_scope.to_owned(),
            refresh_token: None,
            id_token: None,
        }),
    )
        .into_response()
}

async fn handle_refresh_token(
    state: AppState,
    client: oauth::OAuthClient,
    form: TokenRequest,
) -> axum::response::Response {
    let Some(refresh_token) = form.refresh_token.as_deref() else {
        return oauth_error_response(OAuthError::InvalidRequest("refresh_token"));
    };

    let (stored_refresh, rotated_refresh_token) =
        match oauth::rotate_refresh_token(&state.db_pool, &client, refresh_token).await {
            Ok(result) => result,
            Err(error) => return oauth_error_response(error),
        };

    let subject = stored_refresh
        .user_id
        .map(|user_id| user_id.to_string())
        .unwrap_or_else(|| client.client_id.clone());
    let access_token = match state.token_service.issue_access_token(AccessTokenParams {
        subject: &subject,
        audience: &client.client_id,
        scope: &stored_refresh.scope,
        roles: None,
        client_id: Some(&client.client_id),
        authorized_party: Some(&client.client_id),
        ttl_seconds: ACCESS_TOKEN_TTL_SECONDS,
    }) {
        Ok(token) => token,
        Err(_) => return oauth_error_response(OAuthError::InvalidRequest("server_error")),
    };

    (
        StatusCode::OK,
        axum::Json(TokenResponse {
            access_token,
            token_type: "Bearer",
            expires_in: ACCESS_TOKEN_TTL_SECONDS,
            scope: stored_refresh.scope,
            refresh_token: Some(rotated_refresh_token.plain_text),
            id_token: None,
        }),
    )
        .into_response()
}

fn oauth_error_response(error: OAuthError) -> axum::response::Response {
    let (status, code) = match error {
        OAuthError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
        OAuthError::InvalidClient => (StatusCode::UNAUTHORIZED, "invalid_client"),
        OAuthError::InvalidGrant => (StatusCode::BAD_REQUEST, "invalid_grant"),
        OAuthError::UnauthorizedClient => (StatusCode::BAD_REQUEST, "unauthorized_client"),
        OAuthError::UnsupportedGrantType => (StatusCode::BAD_REQUEST, "unsupported_grant_type"),
    };

    (status, axum::Json(OAuthErrorResponse { error: code })).into_response()
}
