//! Business logic and service layer for the admin server.
//!
//! This module contains all business logic, orchestrating operations across
//! repositories and implementing domain-specific rules and workflows.
//!
//! ## Module Layer: Business Logic (Layer 4)
//!
//! This module implements the core business logic of the application.
//!
//! ## Dependencies
//! - **Allowed**: repositories (Layer 3), models (Layer 2), errors (Layer 1), utils (Layer 1), external crates
//! - **Forbidden**: auth (same layer), config (higher layer), middleware (higher layer), handlers (higher layer)
//!
//! ## Dependents
//! - auth (for authentication services)
//! - config (for service initialization)
//! - handlers (for API endpoint logic)
//!
//! ## Responsibility
//! - Implement business rules and validation
//! - Orchestrate operations across multiple repositories
//! - Handle complex workflows and transactions
//! - Provide service-level caching
//! - Implement access control and authorization logic
//! - Manage policy updates and event broadcasting
//! - Generate metrics and analytics
//!
//! ## Design Principles
//! - Services should be stateless (except for injected dependencies)
//! - Each service handles one domain area or aggregate
//! - Services coordinate repositories but don't access database directly
//! - Business logic belongs in services, not repositories or handlers
//!
//! ## Submodules
//! - `access_control`: RBAC and ABAC authorization logic with caching and resilience
//! - `access_evaluation_log`: Access decision logging and audit trail
//! - `history`: Audit trail and history tracking
//! - `metrics`: Application metrics and monitoring
//! - `permission`: Permission management
//! - `policy`: Policy update management and event listeners (moved from config)
//! - `user`: User management
//! - `role`: Role management
//!
//! ## Recent Refactoring Changes
//! This module was expanded to include business logic previously misplaced in config:
//! - **Moved from config/**: `policy_update_manager.rs` and `policy_listeners.rs` consolidated into `policy/` submodule
//!   (policy update management is business logic, not configuration)
//! - The policy submodule now handles event broadcasting, listener management, and policy update coordination

pub mod access_control;
pub mod access_evaluation_log;
pub mod history;
pub mod metrics;
pub mod permission;
pub mod policy;
pub mod user;
pub mod role;
