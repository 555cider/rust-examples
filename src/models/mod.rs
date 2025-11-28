//! Data models and domain entities for the admin server.
//!
//! This module defines the core data structures used throughout the application,
//! including database entities and data transfer objects (DTOs) for API communication.
//!
//! ## Module Layer: Domain (Layer 2)
//!
//! This module defines the domain model and data structures.
//!
//! ## Dependencies
//! - **Allowed**: errors (Layer 1), utils (Layer 1), external crates
//! - **Forbidden**: repositories, services, auth, config, middleware, handlers
//!
//! ## Dependents
//! - repositories (for database operations)
//! - services (for business logic)
//! - auth (for user models)
//! - handlers (for request/response DTOs)
//!
//! ## Responsibility
//! - Define database entity structures
//! - Define API request/response DTOs
//! - Provide data validation rules
//! - Define domain-specific types and enums
//! - Implement serialization/deserialization
//!
//! ## Submodules
//! - `entities`: Database entity models (AdminUser, Permission, UserType, etc.)
//! - `dto`: Data Transfer Objects for API communication

pub mod dto;
pub mod entities;
