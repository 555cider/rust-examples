use std::future::{Future, pending};

use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{config::Config, db, error::Result, http::router};

pub fn build_router() -> axum::Router {
    router::build_router().layer(TraceLayer::new_for_http())
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

    let _db_pool = db::connect(&config.database_url, config.database_max_connections).await?;
    db::migrate(&_db_pool).await?;
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("auth server listening on {}", listener.local_addr()?);

    axum::serve(listener, build_router())
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
