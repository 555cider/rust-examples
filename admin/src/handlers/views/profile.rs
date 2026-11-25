use crate::{errors::AppError, middleware::UserId, AppState};
use axum::{
    extract::{Extension, State},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::sync::Arc;
use tera::Context;
use tracing::{debug, error, info};

pub fn route() -> Router<Arc<AppState>> {
    Router::new().route("/", get(profile_page))
}

async fn profile_page(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    info!("Starting profile page handler for user_id: {}", user_id.0);

    // Get current user data with detailed error handling
    let current_user = match state.service.user_service.get_user_by_id(user_id.0).await {
        Ok(user) => {
            debug!("Found user: {:?}", user);
            Some(user)
        }
        Err(e) => {
            error!("Error fetching user: {}", e);
            None
        }
    };

    // Create Tera context directly
    let mut tera_context = Context::new();

    // Add basic fields
    tera_context.insert("title", "프로필");
    tera_context.insert("active_page", "profile");
    tera_context.insert("user_id", &user_id.0);

    // Add current user if available
    if let Some(user) = &current_user {
        let user_value = serde_json::to_value(user).map_err(|e| {
            error!("Failed to serialize user: {}", e);
            AppError::InternalServerError("Failed to serialize user data".to_string())
        })?;

        tera_context.insert("current_user", &user_value);
    } else {
        // Insert an empty object if no user is available
        tera_context.insert("current_user", &serde_json::json!({}));
    }

    // Debug: Log context information
    debug!(
        "Template context created with title: {}",
        tera_context.get("title").unwrap_or(&tera::Value::Null)
    );

    // Get template names for debugging
    let template_names: Vec<&str> = state.tera.get_template_names().collect();
    debug!("Available templates: {:?}", template_names);

    // Check if template exists
    if !template_names.contains(&"pages/profile/index.html") {
        error!("Template 'pages/profile/index.html' not found in the loaded templates");
        return Err(AppError::InternalServerError(
            "Template not found".to_string(),
        ));
    }

    // Try to render the template
    match state.tera.render("pages/profile/index.html", &tera_context) {
        Ok(html) => {
            debug!("Successfully rendered template");
            Ok(Html(html).into_response())
        }
        Err(e) => {
            // Log detailed error information
            error!("Template rendering error: {}", e);

            // Log the error source if available
            use std::error::Error;
            if let Some(source) = e.source() {
                error!("Source error: {}", source);
            }

            // Log the error as a string for debugging
            let error_string = e.to_string();
            error!("Error details: {}", error_string);

            // Create a simple error response
            Err(AppError::InternalServerError(format!(
                "Template rendering error: {}",
                error_string
            )))
        }
    }
}
