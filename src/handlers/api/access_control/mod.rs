mod config;
mod evaluation;
mod health;
mod access_evaluation_log;
mod policies;
mod policy_updates;

use crate::config::AppState;
use axum::Router;
use std::sync::Arc;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .nest("/config", config::route())
        .nest("/evaluation", evaluation::route())
        .nest("/health", health::route())
        .nest("/logs", access_evaluation_log::route())
        .nest("/policies", policies::route())
        .nest("/policy-updates", policy_updates::route())
}
