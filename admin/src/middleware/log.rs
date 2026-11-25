//! Logging middleware for HTTP requests.
//!
//! This module provides middleware for logging HTTP request details such as method, URI,
//! status code, and response time.
use axum::{http::Request, middleware::Next, response::Response};
use std::time::Instant;
use tracing::{debug, info, warn};

/// Middleware that logs HTTP request details including method, URI, status code, and response time.
///
/// # Example
/// ```
/// use axum::{routing::get, Router};
/// use admin_server::middleware::log;
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "Hello, World!" }))
///     .layer(axum::middleware::from_fn(log));
/// ```
pub async fn log(req: Request<axum::body::Body>, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let start = Instant::now();

    // Process the request
    let response = next.run(req).await;

    // Log the request details
    let status = response.status();
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis();

    // Log at appropriate level based on status code
    // Log at appropriate level based on status code
    if status.is_server_error() {
        warn!(
            method = %method,
            uri = %uri,
            status = %status,
            elapsed_ms = %elapsed_ms,
            "HTTP Request - Server Error"
        );
    } else if status.is_client_error() {
        warn!(
            method = %method,
            uri = %uri,
            status = %status,
            elapsed_ms = %elapsed_ms,
            "HTTP Request - Client Error"
        );
    } else {
        // For static assets and favicon, log at debug level to reduce noise
        let path = uri.path();
        if path.starts_with("/static") || path.starts_with("/favicon.ico") {
            debug!(
                method = %method,
                uri = %uri,
                status = %status,
                elapsed_ms = %elapsed_ms,
                "HTTP Request (Static)"
            );
        } else {
            info!(
                method = %method,
                uri = %uri,
                status = %status,
                elapsed_ms = %elapsed_ms,
                "HTTP Request"
            );
        }
    }

    response
}
