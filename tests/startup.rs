use std::net::{SocketAddr, TcpListener};

use tokio::sync::oneshot;

fn reserve_local_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("ephemeral port should bind");
    let addr = listener.local_addr().expect("listener should have an addr");
    drop(listener);
    addr
}

#[tokio::test]
async fn app_starts_and_serves_health() {
    let bind_addr = reserve_local_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let server = tokio::spawn(auth_server::app::run_with_config_and_shutdown(
        auth_server::config::Config {
            bind_addr,
            database_url: std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set for startup test"),
            database_max_connections: 5,
        },
        async {
            let _ = shutdown_rx.await;
        },
    ));

    let client = reqwest::Client::new();
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match client
                .get(format!("http://{bind_addr}/healthz"))
                .send()
                .await
            {
                Ok(response) => break response,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .expect("server should become reachable");

    assert!(response.status().is_success());
    assert_eq!(
        response
            .json::<serde_json::Value>()
            .await
            .expect("health response should be json"),
        serde_json::json!({ "status": "ok" })
    );

    shutdown_tx
        .send(())
        .expect("server shutdown signal should succeed");
    server
        .await
        .expect("server task should join")
        .expect("server should shut down cleanly");
}
