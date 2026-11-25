mod access_control;
mod access_evaluation_log;
mod auth;
mod dashboard;
mod history;
mod permission;
mod policy;
mod profile;
mod settings;
mod user;
mod role;

use crate::config::AppState;
use axum::Router;
use std::sync::Arc;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        // Auth routes (no auth middleware)
        .nest("/auth", auth::route())
        // Protected routes (all require auth)
        .merge(
            Router::new()
                .merge(dashboard::route()) // GET /
                .nest("/users", user::route()) // /users/*
                .nest("/permissions", permission::route()) // /permissions/*
                .nest("/policies", policy::route()) // /policies/*
                .nest("/roles", role::route()) // /roles/*
                .nest("/history", history::route()) // /history/*
                .nest("/profile", profile::route()) // /profile/*
                .nest("/settings", settings::route()) // /settings/*
                .nest("/access-logs", access_evaluation_log::route()) // /access-logs/*
                .nest("/access-control", access_control::route()) // /access-control/*
                .layer(axum::middleware::from_fn(crate::middleware::auth)),
        )
}
