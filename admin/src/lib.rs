//! Admin Server Library
//!
//! This library provides the core modules for the admin server,
//! making them available for integration tests and other binaries.

pub mod auth;
pub mod config;
pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod repositories;
pub mod services;
pub mod utils;

// Re-export AppState for convenience (backward compatibility)
pub use config::AppState;
