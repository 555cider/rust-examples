use crate::{
    auth::models::AuthnUser,
    errors::AppError,
    utils::{cookies, headers, tokens},
    AppState,
};
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{Html, IntoResponse, Response},
};
use std::{convert::Infallible, sync::Arc};
use tracing::warn;

/// Wrapper type for user ID that implements Send + Sync + 'static
#[derive(Debug, Clone)]
pub struct UserId(pub i64);

impl UserId {
    /// Creates a new UserId with the given user ID.
    pub fn new(user_id: i64) -> Self {
        Self(user_id)
    }
}

/// Renders an unauthorized access page for web requests
async fn render_unauthorized_page() -> Response {
    // Create a simple error page
    let html = r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>Unauthorized</title>
        <script src="https://cdn.tailwindcss.com"></script>
    </head>
    <body class="bg-gray-100 flex items-center justify-center min-h-screen">
        <div class="bg-white p-8 rounded-lg shadow-md max-w-md w-full">
            <div class="text-center">
                <h1 class="text-2xl font-bold text-red-600 mb-4">Unauthorized Access</h1>
                <p class="text-gray-700 mb-6">You need to be logged in to access this page.</p>
                <a href="/auth/login" class="bg-blue-600 text-white px-4 py-2 rounded hover:bg-blue-700 transition-colors">
                    Go to Login
                </a>
            </div>
        </div>
    </body>
    </html>
    "#;

    (StatusCode::UNAUTHORIZED, Html(html)).into_response()
}

/// Middleware that authenticates requests using a JWT token from the Authorization header or access_token cookie.
/// Returns an error if the token is missing, invalid, or expired.
/// For web requests, renders an unauthorized page. For API requests, returns a JSON error.
pub async fn auth(mut request: Request, next: Next) -> Response {
    let headers = request.headers();
    // Check if this is an API request
    let is_api_request = headers::is_api_request(headers);

    // 1. Extract token from Authorization header
    let token = headers::extract_token_from_header(headers)
        // 2. If not found in header, try to get from access_token cookie
        .map(|s| s.to_string())
        .or_else(|| cookies::get_access_token(None, headers));

    let token = match token {
        Some(token) if !token.is_empty() => token,
        _ => {
            warn!("Missing or invalid Authorization header and access_token cookie");
            return if is_api_request {
                AppError::Unauthorized(
                    "Missing or invalid Authorization header or access_token cookie. Please login again."
                        .to_string(),
                )
                    .into_response()
            } else {
                render_unauthorized_page().await
            };
        }
    };

    // Validate the token
    match tokens::validate_token(&token) {
        Ok(claims) => {
            // Add the user ID to the request extensions for use in handlers
            request.extensions_mut().insert(UserId::new(claims.sub));
            next.run(request).await
        }
        Err(e) => {
            warn!(error = %e, "Token validation failed");
            if is_api_request {
                AppError::Unauthorized("Invalid or expired token".to_string()).into_response()
            } else {
                render_unauthorized_page().await
            }
        }
    }
}

/// Middleware that loads the authenticated user data based on the UserId from the auth middleware.
/// This middleware should be applied after the auth middleware to ensure UserId is available.
pub async fn auth_user(mut request: Request<axum::body::Body>, next: Next) -> Response {
    // The auth middleware should have already set the UserId
    if let Some(user_id) = request.extensions().get::<UserId>() {
        if let Some(state) = request.extensions().get::<Arc<AppState>>() {
            match state.service.auth_service.get_authn_user(user_id.0).await {
                Ok(authn_user) => {
                    request.extensions_mut().insert::<AuthnUser>(authn_user);
                }
                Err(e) => {
                    warn!("Failed to load authenticated user data: {}", e);
                    // Continue without user data - the auth middleware already validated the token
                }
            }
        } else {
            warn!("AppState not found in request extensions");
        }
    } else {
        warn!("UserId not found in request extensions - auth middleware may not have run");
    }

    next.run(request).await
}

/// Middleware that optionally authenticates requests using a JWT token from the Authorization header or access_token cookie.
/// If the token is valid, the user ID is added to the request extensions.
/// If the token is missing or invalid, the request continues without authentication.
#[allow(dead_code)]
pub async fn optional_auth(
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, Infallible> {
    // Clone the headers to avoid moving the request
    let headers = request.headers().clone();

    // 1. Try to extract token from Authorization header
    let token = headers::extract_token_from_header(&headers)
        // 2. If not found, try to get from access_token cookie
        .map(|s| s.to_string())
        .or_else(|| cookies::get_access_token(None, &headers));

    // 3. If token exists, validate it
    if let Some(token_str) = token {
        match tokens::validate_token(&token_str) {
            Ok(claims) => {
                // Insert the user ID into request extensions
                request.extensions_mut().insert(UserId::new(claims.sub));
            }
            Err(e) => {
                warn!(error = %e, "Token validation failed");
                // Continue without authentication
            }
        }
    }

    // 4. Continue to the next middleware/handler without authentication
    Ok(next.run(request).await)
}
