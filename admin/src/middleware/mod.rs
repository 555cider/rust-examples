//! HTTP middleware for the admin server.
//!
//! This module contains middleware components that process HTTP requests and responses,
//! providing cross-cutting concerns such as logging, authentication, and error handling.
//!
//! ## Module Layer: Interface (Layer 6)
//!
//! This module provides middleware that intercepts and processes HTTP requests/responses.
//!
//! ## Dependencies
//! - **Allowed**: auth (Layer 4), config (Layer 5), services (Layer 4), repositories (Layer 3), models (Layer 2), errors (Layer 1), utils (Layer 1), external crates
//! - **Forbidden**: handlers (same layer, but middleware should not depend on handlers)
//!
//! ## Dependents
//! - handlers (middleware is applied to handler routes)
//! - main (middleware is configured during application setup)
//!
//! ## Responsibility
//! - Request/response logging
//! - Authentication and authorization enforcement
//! - Error handling and formatting
//! - Request timing and metrics
//! - CORS and security headers
//! - Rate limiting
//!
//! ## Design Principles
//! - Middleware should be composable and reusable
//! - Each middleware should have a single responsibility
//! - Middleware should not contain business logic
//! - Middleware should be order-independent where possible
//!
//! ## Submodules
//! - `auth`: Authentication and authorization middleware (moved from auth/middleware.rs)
//! - `log`: Request/response logging middleware
//!
//! ## Exported Functions
//! - `auth`: Requires authenticated user, returns 401 if not authenticated
//! - `auth_user`: Requires authenticated user, returns 403 if not authenticated
//! - `optional_auth`: Allows both authenticated and unauthenticated requests
//! - `log`: Logs request method, path, and response status
//!
//! ## Recent Refactoring Changes
//! This module was reorganized to consolidate all middleware in one location:
//! - **Moved from auth/**: `middleware.rs` renamed to `auth.rs` (middleware belongs with other middleware)
//! - All authentication/authorization middleware now lives in this module for better discoverability

pub mod auth;
pub mod log;

pub use auth::{auth, auth_user, optional_auth, UserId};
pub use log::log;
