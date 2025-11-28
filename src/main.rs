//! # Admin Server
//!
//! A production-ready Rust admin server with hybrid RBAC/ABAC access control,
//! JWT authentication, and comprehensive security features.
//!
//! ## Features
//! - JWT-based authentication with refresh tokens
//! - Hybrid RBAC/ABAC access control system
//! - Circuit breaker pattern for resilience
//! - Multi-level caching system
//! - OAuth2 support
//! - Comprehensive audit logging
//! - Security-first design
//!
//! ## Security
//! See SECURITY.md for detailed security considerations and best practices.

// Import AppState from the library crate
use admin_server::AppState;

use admin_server::{config, handlers, middleware, services, utils};

use config::{
    access_control::AccessControlConfigManager, container::ServiceContainer, database,
    env_loader::AppConfig, logger,
};
use services::{
    access_control::resilience::{
        AccessControlErrorHandler, CircuitBreakerConfig, DegradationStrategy,
    },
    policy::{
        initialize_advanced_listeners, initialize_policy_listeners,
        start_policy_update_background_task, PolicyUpdateManager,
    },
};

use anyhow::Context;
use axum::{http::HeaderName, Router};
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer, sensitive_headers::SetSensitiveRequestHeadersLayer,
};

use utoipa::OpenApi;

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Failed to initialize tokio runtime")?;

    if let Err(e) = rt.block_on(async_main()) {
        eprintln!("Application error: {:#?}", e);
        std::process::exit(1);
    }

    Ok(())
}

async fn async_main() -> anyhow::Result<()> {
    // Load configuration
    let config = AppConfig::from_env();

    // Initialize logger with configuration
    logger::init_logger(&config);
    tracing::info!("🚀 어드민 서버를 시작합니다.");
    tracing::info!("Server version: {}", env!("CARGO_PKG_VERSION"));

    // Initialize global log redactor with security config
    let security_config = config::security::SecurityConfig::from_env();
    utils::log_redaction::init_global_redactor(&security_config);

    // Run database migrations
    tracing::info!("🔄 데이터베이스 마이그레이션을 실행합니다...");
    database::run_migrations(&config.database_url)
        .await
        .context("데이터베이스 마이그레이션 실패")?;
    tracing::info!("✅ 데이터베이스 마이그레이션이 완료되었습니다.");

    // Initialize database connection pool
    let db_pool = database::setup_db_pool(&config.database_url).await;

    // Initialize template engine
    let tera = match utils::templates::setup_tera() {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("기본 템플릿 로딩 실패: {}", e);
            tracing::info!("대체 템플릿 로더를 시도합니다...");
            utils::templates::setup_tera_with_fallback()?
        }
    };

    let tera_arc = Arc::new(tera);

    // Initialize service container
    let service = ServiceContainer::new(Arc::from(db_pool.clone()));

    // Initialize access control configuration manager
    let access_control_config = Arc::new(AccessControlConfigManager::new(
        config.access_control.clone(),
    ));

    // Initialize policy update manager
    let policy_update_manager = Arc::new(PolicyUpdateManager::default());

    // Initialize policy update listeners
    if let Err(e) = initialize_policy_listeners(Arc::clone(&policy_update_manager)).await {
        tracing::warn!("Failed to initialize policy listeners: {}", e);
    }

    // Initialize advanced policy listeners
    if let Err(e) = initialize_advanced_listeners(Arc::clone(&policy_update_manager)).await {
        tracing::warn!("Failed to initialize advanced policy listeners: {}", e);
    }

    // Start background task for policy updates
    if let Err(e) = start_policy_update_background_task(Arc::clone(&policy_update_manager)).await {
        tracing::warn!("Failed to start policy update background task: {}", e);
    }

    // Create access control error handler
    let access_control_error_handler = Arc::new(AccessControlErrorHandler::new(
        DegradationStrategy::FallbackToRbac,
        CircuitBreakerConfig::default(),
    ));

    // Create application state wrapped in Arc
    let app_state = Arc::new(AppState {
        config,
        pool: db_pool,
        tera: tera_arc,
        service,
        access_control_config,
        policy_update_manager,
        access_control_error_handler,
    });

    // Configure router and middleware
    let app = Router::new()
        .merge(handlers::route())
        .with_state(Arc::clone(&app_state))
        .layer(CompressionLayer::new()) // Add compression layer
        .layer(SetSensitiveRequestHeadersLayer::new(vec![
            HeaderName::from_static("authorization"),
        ]))
        // Only apply logging middleware globally - authentication is handled per-route
        .layer(axum::middleware::from_fn(middleware::log));

    // OpenAPI documentation
    #[cfg(feature = "swagger-ui")]
    let app = {
        #[derive(utoipa::OpenApi)]
        #[openapi(
            paths(
                // Auth
                handlers::api::auth::post_auth_login,
                handlers::api::auth::post_auth_refresh,
                handlers::api::auth::post_auth_register,
                handlers::api::auth::post_auth_logout,
                handlers::api::auth::get_auth_me,
            ),
            components(
                schemas(
                    admin_server::models::dto::auth::LoginRequest,
                    admin_server::models::dto::auth::LoginResponse,
                    admin_server::models::dto::auth::RegisterRequest,
                    admin_server::models::dto::auth::CurrentUserResponse,
                    admin_server::models::dto::role::RoleResponse,
                )
            ),
            tags(
                (name = "auth", description = "Authentication endpoints")
            )
        )]
        struct ApiDoc;

        app.merge(utoipa_swagger_ui::SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
    };

    // Start HTTP server
    let addr = SocketAddr::from(([0, 0, 0, 0], app_state.config.server_port));
    tracing::info!("🌐 서버 시작 중: http://{}", addr);

    let listener = TcpListener::bind(addr)
        .await
        .context("서버 소켓 바인딩 실패")?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(utils::shutdown::shutdown_signal())
    .await?;

    Ok(())
}
