use crate::AppState;
use axum::{
    extract::{Extension, State},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::sync::Arc;

pub fn route() -> Router<Arc<AppState>> {
    Router::new().route("/simulator", get(simulator_page))
}

async fn simulator_page(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<crate::middleware::UserId>,
) -> impl IntoResponse {
    let mut context = tera::Context::new();
    context.insert("active_page", "simulator");
    context.insert("user_id", &user_id.0);

    // Fetch current user
    if let Ok(user) = state.service.user_service.get_user_by_id(user_id.0).await {
        context.insert("current_user", &user);
    }
    
    let html = state
        .tera
        .render("pages/access_control/simulator.html", &context)
        .unwrap_or_else(|e| {
            tracing::error!("Template rendering error: {}", e);
            format!("Error rendering template: {}", e)
        });
        
    Html(html)
}
