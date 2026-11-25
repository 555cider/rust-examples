//! Policy update management and listeners.
//!
//! This module provides real-time policy update management, event broadcasting,
//! and listener registration for handling policy changes throughout the system.
//!
//! ## Components
//! - `update_manager`: Core policy update manager with event broadcasting
//! - `listeners`: Default and custom policy update listeners
//!
//! ## Usage
//! ```rust
//! use admin_server::services::policy::{PolicyUpdateManager, initialize_policy_listeners};
//! use std::sync::Arc;
//!
//! // Create policy update manager
//! let manager = Arc::new(PolicyUpdateManager::default());
//!
//! // Initialize default listeners
//! initialize_policy_listeners(manager.clone()).await?;
//!
//! // Notify policy changes
//! manager.notify_policy_updated("policy1".to_string(), "Updated Policy".to_string()).await?;
//! ```

pub mod listeners;
pub mod update_manager;

pub use update_manager::{
    AuditLoggingListener, CacheInvalidationListener, PolicyUpdateEvent, PolicyUpdateListener,
    PolicyUpdateManager,
};

pub use listeners::{
    initialize_advanced_listeners, initialize_policy_listeners,
    start_policy_update_background_task, DatabaseSyncListener, MetricsCollectionListener,
};
