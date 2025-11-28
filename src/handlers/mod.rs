//! HTTP request handlers for the admin server.
//!
//! This module contains all HTTP endpoint handlers, including both API endpoints
//! and view/template rendering endpoints.
//!
//! ## Module Layer: Interface (Layer 7)
//!
//! This is the top layer that handles HTTP requests and responses.
//!
//! ## Dependencies
//! - **Allowed**: All lower layers - middleware (Layer 6), config (Layer 5), auth (Layer 4), services (Layer 4), repositories (Layer 3), models (Layer 2), errors (Layer 1), utils (Layer 1), external crates
//! - **Forbidden**: None (this is the top layer)
//!
//! ## Dependents
//! - main (handlers are registered during application setup)
//!
//! ## Responsibility
//! - Parse and validate HTTP requests
//! - Extract request parameters and body
//! - Call appropriate services to handle business logic
//! - Format responses (JSON for API, HTML for views)
//! - Handle HTTP-specific concerns (status codes, headers)
//! - Apply authentication and authorization checks
//!
//! ## Design Principles
//! - Handlers should be thin - delegate to services for business logic
//! - Each handler should handle one endpoint or related group of endpoints
//! - Handlers should not contain business logic
//! - Handlers should validate input and format output only
//! - Use DTOs for request/response serialization
//!
//! ## Submodules
//! - `api`: REST API endpoints (JSON responses)
//!   - `access_control`: Access control management endpoints
//!   - `auth`: Authentication endpoints (login, logout, register)
//!   - `dashboard`: Dashboard data endpoints
//!   - `permission`: Permission management endpoints
//!   - `user`: User management endpoints
//!   - `role`: Role management endpoints
//! - `views`: HTML template rendering endpoints
//!   - View handlers for web UI pages

pub mod api;
mod views;

use crate::config::AppState;
use axum::Router;
use std::sync::Arc;
use tower_http::services::ServeDir;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .nest_service("/static", ServeDir::new("static"))
        .nest("/api", api::route())
        .merge(views::route())
}
