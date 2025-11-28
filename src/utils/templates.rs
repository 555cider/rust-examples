use crate::errors::AppError;
use anyhow::{anyhow, Result};
use axum::response::IntoResponse;
use serde::Serialize;
use tera::{Context, Tera};
use tracing::{error, info, warn};

/// Helper function to render templates with comprehensive error handling
pub async fn render_template<T: Serialize + std::fmt::Debug>(
    tera: &Tera,
    template_name: &str,
    context: T,
) -> Result<axum::response::Response, AppError> {
    // Check if template exists
    let available_templates: Vec<&str> = tera.get_template_names().collect();
    if !available_templates.contains(&template_name) {
        error!(
            "Template '{}' not found. Available templates: {:?}",
            template_name, available_templates
        );

        // Try to render error template as fallback
        if template_name != "pages/error.html" && available_templates.contains(&"pages/error.html") {
            let error_context = Context::new();
            match tera.render("pages/error.html", &error_context) {
                Ok(html) => return Ok(axum::response::Html(html).into_response()),
                Err(e) => error!("Failed to render error template fallback: {}", e),
            }
        }

        return Err(AppError::InternalServerError(format!(
            "Template '{}' not found. Available templates: {:?}",
            template_name, available_templates
        )));
    }

    // Convert context to Tera Context with better error handling
    let tera_context = Context::from_serialize(&context).map_err(|e| {
        error!(
            "Failed to serialize context for template '{}': {}",
            template_name, e
        );
        error!("Context data: {:?}", context);
        AppError::InternalServerError(format!(
            "Failed to create template context for '{}': {}",
            template_name, e
        ))
    })?;

    // Render the template with detailed error information
    match tera.render(template_name, &tera_context) {
        Ok(html) => Ok(axum::response::Html(html).into_response()),
        Err(e) => {
            error!("Failed to render template '{}': {}", template_name, e);
            error!("Template rendering error details: {:#?}", e);

            // Try to render a simple error page as fallback
            if template_name != "pages/error.html" && available_templates.contains(&"pages/error.html") {
                let mut error_context = Context::new();
                error_context.insert(
                    "error_message",
                    &format!("Template rendering failed: {}", template_name),
                );
                error_context.insert("title", "Template Error");

                match tera.render("pages/error.html", &error_context) {
                    Ok(html) => {
                        warn!(
                            "Rendered error template as fallback for '{}'",
                            template_name
                        );
                        return Ok(axum::response::Html(html).into_response());
                    }
                    Err(fallback_err) => {
                        error!("Failed to render error template fallback: {}", fallback_err);
                    }
                }
            }

            Err(AppError::InternalServerError(format!(
                "Failed to render template '{}': {}",
                template_name, e
            )))
        }
    }
}

/// Initialize and configure Tera template engine with glob pattern support
///
/// # Errors
///
/// Returns an error if template loading or parsing fails
pub fn setup_tera() -> Result<Tera> {
    // Try to find the templates directory
    let template_dir = std::env::current_dir()?.join("static/templates");

    if !template_dir.exists() {
        error!(
            "Templates directory not found at: {}",
            template_dir.display()
        );
        return Err(anyhow!(
            "Templates directory not found at: {}",
            template_dir.display()
        ));
    }

    info!("Loading templates from: {}", template_dir.display());

    // Load all HTML files in the templates directory
    let main_glob = format!("{}/**/*.html", template_dir.display());

    // Use parse_glob to load all templates including subdirectories
    let mut tera = match Tera::new(&main_glob) {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to parse templates from {}: {}", main_glob, e);
            return Err(anyhow!("Failed to parse templates: {}", e));
        }
    };

    // Configure auto-escaping for HTML files
    tera.autoescape_on(vec![".html"]);

    // Verify that essential templates are loaded
    let template_names: Vec<&str> = tera.get_template_names().collect();
    info!(
        "Successfully loaded {} templates: {:?}",
        template_names.len(),
        template_names
    );

    // Check for essential templates
    let essential_templates = [
        "layouts/base.html",
        "layouts/base_auth.html",
        "pages/auth/login.html",
        "pages/dashboard/index.html",
        "pages/error/index.html",
    ];

    for template in &essential_templates {
        if !template_names.contains(template) {
            warn!("Essential template '{}' not found", template);
        }
    }

    Ok(tera)
}

/// A more detailed version that allows for custom template loading logic with fallbacks
///
/// # Errors
///
/// Returns an error if template loading or parsing fails
pub fn setup_tera_with_fallback() -> Result<Tera> {
    // First try the standard way
    match setup_tera() {
        Ok(tera) => Ok(tera),
        Err(e) => {
            tracing::warn!("Standard template loading failed: {}", e);

            // Try multiple possible template directory locations
            let possible_paths = [
                std::env::current_dir()?.join("static/templates"),
                std::env::current_dir()?.join("../static/templates"),
                std::env::current_dir()?.join("../../static/templates"),
            ];

            // Find the first path that exists and is a directory
            let templates_dir = possible_paths
                .iter()
                .find(|path| path.is_dir())
                .ok_or_else(|| {
                    anyhow!(
                        "Could not find templates directory in fallback mode. Tried: {:?}",
                        possible_paths
                    )
                })?;

            tracing::info!(
                "Attempting fallback template loading from: {}",
                templates_dir.display()
            );

            // Create a new Tera instance with the templates directory as a template root
            let template_glob = format!("{}/**/*.html", templates_dir.display());
            let mut tera = match Tera::new(&template_glob) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("Failed to parse templates: {}", e);
                    return Err(anyhow!("Failed to parse templates: {}", e));
                }
            };

            // Enable autoescaping for HTML safety
            tera.autoescape_on(vec![".html"]);

            // Verify base template is loaded
            if !tera.get_template_names().any(|name| name == "layouts/base.html") {
                tracing::error!("Base template (layouts/base.html) not found in the templates directory");
                return Err(anyhow!("Base template (layouts/base.html) not found"));
            }

            let loaded_templates: Vec<_> = tera.get_template_names().collect();
            tracing::info!(
                "Successfully loaded {} templates in fallback mode: {:?}",
                loaded_templates.len(),
                loaded_templates
            );

            Ok(tera)
        }
    }
}
