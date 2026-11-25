//! Utility functions and helpers for the admin server.
//!
//! This module provides reusable utility functions for common tasks such as
//! cookie handling, password hashing, template rendering, token management,
//! log redaction, and graceful shutdown.
//!
//! ## Module Layer: Foundation (Layer 1)
//!
//! This is a foundational module that provides utilities used by higher-level modules.
//!
//! ## Dependencies
//! - **Allowed**: errors (Layer 1), external crates
//! - **Forbidden**: models, repositories, services, auth, config, middleware, handlers
//!
//! ## Dependents
//! - models (for validation utilities)
//! - repositories (for data transformation)
//! - services (for business logic utilities)
//! - auth (for password hashing, token generation)
//! - config (for configuration utilities)
//! - middleware (for request/response utilities)
//! - handlers (for template rendering, cookie handling)
//!
//! ## Responsibility
//! - Cookie creation and parsing
//! - HTTP header utilities
//! - Password hashing and verification
//! - Template engine setup and rendering
//! - JWT token generation and validation
//! - Sensitive data redaction in logs
//! - Graceful shutdown signal handling
//!
//! ## Submodules
//! - `cookies`: Cookie creation and parsing utilities
//! - `headers`: HTTP header manipulation helpers
//! - `log_redaction`: Sensitive data redaction for logging
//! - `password`: Password hashing and verification (bcrypt)
//! - `shutdown`: Graceful shutdown signal handling (moved from infrastructure)
//! - `templates`: Template engine setup and rendering (consolidated from config/template.rs)
//! - `tokens`: JWT token generation and validation
//!
//! ## Recent Refactoring Changes
//! This module was expanded to include utilities previously scattered across other modules:
//! - **Moved from infrastructure/**: `shutdown.rs` (graceful shutdown is a utility concern)
//! - **Consolidated from config/**: Template setup functions from `config/template.rs` merged into `templates.rs`
//!   (template setup is a utility, not configuration)

pub mod cookies;
pub mod headers;
pub mod log_redaction;
pub mod password;
pub mod shutdown;
pub mod templates;
pub mod tokens;

pub use templates::render_template;
