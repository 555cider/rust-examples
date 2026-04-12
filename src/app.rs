use std::net::TcpListener;

use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{config::Config, error::Result, http::router};

pub fn build_router() -> axum::Router {
    router::build_router().layer(TraceLayer::new_for_http())
}

pub async fn run() -> Result<()> {
    init_tracing();

    let config = Config::from_env()?;
    let listener = TcpListener::bind(config.bind_addr)?;

    tracing::info!("auth server listening on {}", listener.local_addr()?);

    axum::serve(tokio::net::TcpListener::from_std(listener)?, build_router()).await?;

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
