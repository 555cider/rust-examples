mod access_control;
mod access_evaluation_log;
pub mod auth;
mod dashboard;
mod history;
mod permission;
mod user;
mod role;

use crate::config::AppState;
use axum::Router;
use std::sync::Arc;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .nest("/access-control", access_control::route())
        .nest("/access-logs", access_evaluation_log::route())
        .nest("/auth", auth::route())
        .nest("/dashboard", dashboard::route())
        .nest("/history", history::route())
        .nest("/permissions", permission::route())
        .nest("/users", user::route())
        .nest("/roles", role::route())
}
