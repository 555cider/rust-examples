use axum::{Router, routing::get};

use crate::http::health;

pub fn build_router() -> Router {
    Router::new().route("/healthz", get(health::health_check))
}
