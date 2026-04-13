use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Deserialize;
use time::Duration;

use crate::{
    identity::{password, repository, session},
    state::AppState,
};

pub const SESSION_COOKIE_NAME: &str = "auth_server_session";
const SESSION_TTL_SECONDS: i32 = 60 * 60 * 12;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

pub fn build_router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let username = payload.username.trim();
    if username.is_empty() || payload.password.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let Some(credential) = repository::find_user_password_credential(&state.db_pool, username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let password_is_valid = password::verify_password(&payload.password, &credential.password_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !password_is_valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let session_token = session::issue_session_token();
    repository::create_session(
        &state.db_pool,
        credential.user_id,
        &session_token.hash,
        SESSION_TTL_SECONDS,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        jar.add(build_session_cookie(
            session_token.plain_text,
            state.secure_cookies,
        )),
        StatusCode::NO_CONTENT,
    ))
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, StatusCode> {
    if let Some(cookie) = jar.get(SESSION_COOKIE_NAME) {
        let session_token_hash = session::hash_session_token(cookie.value());
        repository::revoke_session(&state.db_pool, &session_token_hash)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok((
        jar.remove(clear_session_cookie(state.secure_cookies)),
        StatusCode::NO_CONTENT,
    ))
}

fn build_session_cookie(session_token: String, secure_cookies: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE_NAME, session_token);
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(secure_cookies);
    cookie.set_max_age(Duration::seconds(i64::from(SESSION_TTL_SECONDS)));
    cookie
}

fn clear_session_cookie(secure_cookies: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE_NAME, "");
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(secure_cookies);
    cookie.set_max_age(Duration::seconds(0));
    cookie
}
