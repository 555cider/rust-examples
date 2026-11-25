//! Application State Module
//!
//! This module defines the shared application state that is passed to all handlers
//! and middleware throughout the application.

use axum::extract::FromRef;
use std::sync::Arc;
use tera::Tera;

/// Application state shared across all handlers
///
/// This struct contains all the shared resources needed by the application,
/// including configuration, database pool, template engine, services, and
/// access control components.
#[derive(Clone, FromRef)]
pub struct AppState {
    /// Application configuration
    pub config: crate::config::env_loader::AppConfig,

    /// Database connection pool
    pub pool: sqlx::SqlitePool,

    /// Template engine
    pub tera: Arc<Tera>,

    /// Service container with all business logic services
    pub service: crate::config::container::ServiceContainer,

    /// Access control configuration manager
    pub access_control_config: Arc<crate::config::access_control::AccessControlConfigManager>,

    /// Policy update manager for dynamic policy updates
    pub policy_update_manager: Arc<crate::services::policy::PolicyUpdateManager>,

    /// Error handler for access control failures
    pub access_control_error_handler:
        Arc<crate::services::access_control::resilience::AccessControlErrorHandler>,
}
