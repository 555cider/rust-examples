//! Authentication and authorization module.
//!
//! This module contains all authentication-related functionality including:
//! - User authentication service
//! - User models and types
//! - JWT token extraction
//!
//! ## Dependencies
//! - Allowed: models, repositories, services, errors, utils
//! - Forbidden: handlers, config (except for AppConfig)
//!
//! ## Dependents
//! - middleware (authentication middleware has been moved to the middleware module)
//! - handlers (for auth endpoints)

pub mod extractor;
pub mod models;
pub mod service;

// Re-export commonly used types
pub use models::{AuthnUser, User};
pub use service::AuthService;
