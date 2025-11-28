//! Data access layer for the admin server.
//!
//! This module provides repository implementations for database operations,
//! following the repository pattern to abstract data access logic.
//!
//! ## Module Layer: Domain (Layer 3)
//!
//! This module handles all database interactions and data persistence.
//!
//! ## Dependencies
//! - **Allowed**: models (Layer 2), errors (Layer 1), utils (Layer 1), external crates
//! - **Forbidden**: services, auth, config, middleware, handlers
//!
//! ## Dependents
//! - services (for business logic that requires data access)
//! - auth (for authentication-related data access)
//! - config (for initialization and setup)
//!
//! ## Responsibility
//! - Execute database queries (SELECT, INSERT, UPDATE, DELETE)
//! - Map database rows to domain entities
//! - Handle database transactions
//! - Provide data access abstractions
//! - Implement query optimization
//!
//! ## Design Principles
//! - Each repository handles one entity or aggregate
//! - Repositories should not contain business logic
//! - All database errors are converted to AppError
//! - Use prepared statements for SQL injection prevention

pub mod access_evaluation_log;
pub mod auth;
pub mod history;
pub mod permission;
pub mod policy;
pub mod user;
pub mod role;

pub use access_evaluation_log::AccessEvaluationLogRepository;
pub use auth::AuthRepository;
pub use history::HistoryRepository;
pub use permission::PermissionRepository;
pub use policy::PolicyRepository;
pub use user::UserRepository;
pub use role::RoleRepository;
