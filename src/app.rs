use std::future::{Future, pending};

use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    config::Config,
    db,
    error::{AppError, Result},
    http,
    state::AppState,
    token::{SigningKeys, TokenService},
};

pub fn build_router() -> axum::Router {
    http::router::build_router().layer(TraceLayer::new_for_http())
}

pub fn build_application_router(
    db_pool: PgPool,
    secure_cookies: bool,
    issuer: String,
    token_service: TokenService,
) -> axum::Router {
    let state = AppState::new(db_pool, secure_cookies, issuer, token_service);

    axum::Router::<AppState>::new()
        .route("/healthz", axum::routing::get(http::health::health_check))
        .merge(http::admin::build_router())
        .merge(http::auth::build_router())
        .merge(http::oidc::build_router())
        .merge(http::oauth::build_router())
        .merge(http::policy::build_router())
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

pub async fn run() -> Result<()> {
    let config = Config::from_env()?;
    run_with_config_and_shutdown(config, pending()).await
}

pub async fn run_with_config_and_shutdown<F>(config: Config, shutdown: F) -> Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    init_tracing();

    let db_pool = db::connect(&config.database_url, config.database_max_connections).await?;
    db::migrate(&db_pool).await?;
    let signing_keys = load_signing_keys(&config)?;
    let token_service = TokenService::new(config.issuer.clone(), signing_keys);
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("auth server listening on {}", listener.local_addr()?);

    axum::serve(
        listener,
        build_application_router(
            db_pool,
            !config.bind_addr.ip().is_loopback(),
            config.issuer.clone(),
            token_service,
        ),
    )
    .with_graceful_shutdown(shutdown)
    .await?;

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("auth_server=info,tower_http=info"));

    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer())
        .try_init();
}

fn load_signing_keys(config: &Config) -> Result<SigningKeys> {
    match &config.signing_key_path {
        Some(path) => SigningKeys::from_pem_file(path),
        None if config.bind_addr.ip().is_loopback() => Ok(SigningKeys::generate_ephemeral()),
        None => Err(AppError::MissingEnvVar("AUTH_SERVER_SIGNING_KEY_PATH")),
    }
}
