//! Application configuration and infrastructure setup.
//!
//! This module handles application configuration, initialization, and infrastructure
//! concerns such as database setup, logging, and service container management.
//!
//! ## Module Layer: Infrastructure (Layer 5)
//!
//! This module provides infrastructure and configuration for the application.
//!
//! ## Dependencies
//! - **Allowed**: services (Layer 4), auth (Layer 4), repositories (Layer 3), models (Layer 2), errors (Layer 1), utils (Layer 1), external crates
//! - **Forbidden**: middleware (higher layer), handlers (higher layer)
//!
//! ## Dependents
//! - app_state (for application state initialization)
//! - main (for application startup)
//! - middleware (for configuration access)
//! - handlers (for configuration access)
//!
//! ## Responsibility
//! - Load and validate environment configuration
//! - Initialize database connections and run migrations
//! - Configure logging and monitoring
//! - Set up service container with dependency injection
//! - Manage application state
//!
//! ## Design Principles
//! - Configuration should be loaded once at startup
//! - All configuration should be validated early
//! - Services should be initialized in dependency order
//! - Configuration errors should fail fast
//!
//! ## Submodules
//! - `access_control`: Access control policy configuration
//! - `app_state`: Application state definition (moved from src/app_state.rs)
//! - `container`: Service container with dependency injection (moved from infrastructure)
//! - `database`: Database connection setup and migrations (consolidated from infrastructure/migrations.rs)
//! - `env_loader`: Environment variable loading and validation
//! - `logger`: Logging configuration and initialization (moved from infrastructure)
//! - `security`: Security configuration (CORS, rate limiting, etc.)
//!
//! ## Recent Refactoring Changes
//! This module was reorganized to consolidate configuration-related functionality:
//! - **Moved from src/**: `app_state.rs` (application state belongs with configuration)
//! - **Moved from infrastructure/**: `container.rs`, `logger.rs` (infrastructure module removed)
//! - **Consolidated**: `migrations.rs` merged into `database.rs` (related functionality)
//! - **Moved to services/**: `policy_update_manager.rs`, `policy_listeners.rs` (business logic)
//! - **Moved to utils/**: `template.rs` setup functions (utility functionality)
//!
//! ## Backward Compatibility Re-exports
//! The following modules have been moved but are re-exported here for backward compatibility:
//! - `graceful_shutdown` -> `crate::utils::shutdown` (moved from infrastructure to utils)
//! - AppState is re-exported at crate root for convenience

pub mod access_control;
pub mod app_state;
pub mod container;
pub mod database;
pub mod env_loader;
pub mod logger;
pub mod security;

// Re-export AppState for convenience
pub use app_state::AppState;

// Re-export shutdown module for backward compatibility
pub use crate::utils::shutdown as graceful_shutdown;
