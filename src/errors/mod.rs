//! Error types and error handling for the admin server.
//!
//! This module defines all error types used throughout the application and provides
//! consistent error responses for HTTP endpoints.
//!
//! ## Module Layer: Foundation (Layer 1)
//!
//! This is a foundational module that provides error types used by all other modules.
//!
//! ## Dependencies
//! - **Allowed**: External crates only (axum, thiserror, sqlx, etc.)
//! - **Forbidden**: All internal modules (this is a foundation module)
//!
//! ## Dependents
//! All modules in the application can depend on this module:
//! - utils
//! - models
//! - repositories
//! - services
//! - auth
//! - config
//! - middleware
//! - handlers
//!
//! ## Responsibility
//! - Define application-wide error types
//! - Provide error conversion and mapping
//! - Implement HTTP response formatting for errors
//! - Ensure consistent error handling across the application
//!
//! ## Structure
//! This module was consolidated from a split structure (src/errors.rs + src/errors/ directory)
//! into a unified directory structure:
//! - `mod.rs`: Core error types (AppError) and HTTP response formatting
//! - `access_control.rs`: Access control specific errors
//! - `context.rs`: Error context and metadata

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use bcrypt::BcryptError;
use thiserror::Error;
use validator::ValidationErrors;

pub mod access_control;
pub mod context;

pub use access_control::{AccessControlError, ErrorSeverity};

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal server error")]
    InternalServerError(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Password hashing error: {0}")]
    PasswordHashingError(#[from] BcryptError),

    #[error("JWT error: {0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),

    #[error("Validation error")]
    ValidationError(#[from] ValidationErrors),

    #[error("Access control error: {0}")]
    AccessControlError(#[from] AccessControlError),
}

/// Struct for consistent error response format
#[derive(serde::Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    pub status_code: u16,
    pub timestamp: String,
    pub path: Option<String>, // This will be set by middleware if needed
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (error_type, status, message) = match &self {
            AppError::BadRequest(msg) => ("BAD_REQUEST", StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Unauthorized(msg) => ("UNAUTHORIZED", StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::NotFound(msg) => ("NOT_FOUND", StatusCode::NOT_FOUND, msg.clone()),
            AppError::Forbidden(msg) => ("FORBIDDEN", StatusCode::FORBIDDEN, msg.clone()),
            AppError::Conflict(msg) => ("CONFLICT", StatusCode::CONFLICT, msg.clone()),
            AppError::InternalServerError(msg) => (
                "INTERNAL_SERVER_ERROR",
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal server error: {}", msg.clone()),
            ),
            AppError::DatabaseError(e) => (
                "DATABASE_ERROR",
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            ),
            AppError::PasswordHashingError(e) => (
                "PASSWORD_HASHING_ERROR",
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Password hashing error: {}", e),
            ),
            AppError::JwtError(e) => (
                "JWT_ERROR",
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("JWT error: {}", e),
            ),
            AppError::ValidationError(e) => (
                "VALIDATION_ERROR",
                StatusCode::BAD_REQUEST,
                format!("Validation error: {}", e),
            ),
            AppError::AccessControlError(e) => {
                let status = match e.severity() {
                    ErrorSeverity::Low => StatusCode::FORBIDDEN,
                    ErrorSeverity::Medium => StatusCode::BAD_REQUEST,
                    ErrorSeverity::High => StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorSeverity::Critical => StatusCode::SERVICE_UNAVAILABLE,
                };
                (e.error_code(), status, e.to_string())
            }
        };

        let error_response = ErrorResponse {
            error: error_type.to_string(),
            message,
            status_code: status.as_u16(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            path: None, // Will be set by error middleware if needed
        };

        (status, Json(error_response)).into_response()
    }
}
