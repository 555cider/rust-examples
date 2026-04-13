use std::{
    fs,
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use reqwest::StatusCode;
use sqlx::PgPool;
use tokio::{sync::oneshot, task::JoinHandle};

const TEST_SIGNING_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIKsGy9B4I0tmInArgmTzxXb2Y3ahKe19Alib2Gj97X9S\n-----END PRIVATE KEY-----\n";

fn reserve_local_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("ephemeral port should bind");
    let addr = listener.local_addr().expect("listener should have an addr");
    drop(listener);
    addr
}

fn write_signing_key_pem() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be valid")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("auth-server-signing-key-{unique}.pem"));
    fs::write(&path, TEST_SIGNING_KEY_PEM).expect("test signing key should be written");
    path
}

async fn database_url_for_pool(pool: &PgPool) -> String {
    let database_name: String = sqlx::query_scalar("select current_database()")
        .fetch_one(pool)
        .await
        .expect("current database should be available");

    let base_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for discovery tests");
    let mut url = url::Url::parse(&base_url).expect("DATABASE_URL should be a valid URL");
    url.set_path(&format!("/{database_name}"));
    url.to_string()
}

async fn spawn_server(database_url: String) -> (String, oneshot::Sender<()>, JoinHandle<()>) {
    let bind_addr = reserve_local_addr();
    let issuer = format!("http://{bind_addr}");
    let server_issuer = issuer.clone();
    let signing_key_path = write_signing_key_pem();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        auth_server::app::run_with_config_and_shutdown(
            auth_server::config::Config {
                bind_addr,
                database_url,
                database_max_connections: 5,
                issuer: server_issuer,
                signing_key_path: Some(signing_key_path.clone()),
            },
            async {
                let _ = shutdown_rx.await;
            },
        )
        .await
        .expect("server should run cleanly");

        let _ = fs::remove_file(signing_key_path);
    });

    let client = reqwest::Client::new();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match client.get(format!("{issuer}/healthz")).send().await {
                Ok(response) if response.status().is_success() => break,
                _ => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .expect("server should become reachable");

    (issuer, shutdown_tx, handle)
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn openid_configuration_exposes_core_endpoints(pool: PgPool) {
    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;

    let response = reqwest::Client::new()
        .get(format!("{issuer}/.well-known/openid-configuration"))
        .send()
        .await
        .expect("discovery request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(reqwest::header::CACHE_CONTROL),
        Some(&reqwest::header::HeaderValue::from_static(
            "public, max-age=300"
        ))
    );

    let json: serde_json::Value = response.json().await.expect("response should be json");
    assert_eq!(json["issuer"], issuer);
    assert_eq!(json["jwks_uri"], format!("{issuer}/jwks.json"));
    assert_eq!(
        json["authorization_endpoint"],
        format!("{issuer}/authorize")
    );
    assert_eq!(json["token_endpoint"], format!("{issuer}/token"));
    assert_eq!(json["userinfo_endpoint"], format!("{issuer}/userinfo"));
    assert_eq!(
        json["id_token_signing_alg_values_supported"],
        serde_json::json!(["EdDSA"])
    );

    shutdown_tx
        .send(())
        .expect("shutdown signal should be delivered");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn jwks_endpoint_exposes_single_eddsa_signing_key(pool: PgPool) {
    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;

    let response = reqwest::Client::new()
        .get(format!("{issuer}/jwks.json"))
        .send()
        .await
        .expect("jwks request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(reqwest::header::CACHE_CONTROL),
        Some(&reqwest::header::HeaderValue::from_static(
            "public, max-age=300"
        ))
    );

    let json: serde_json::Value = response.json().await.expect("response should be json");
    let keys = json["keys"].as_array().expect("jwks should contain keys");
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["kty"], "OKP");
    assert_eq!(keys[0]["crv"], "Ed25519");
    assert_eq!(keys[0]["alg"], "EdDSA");
    assert_eq!(keys[0]["use"], "sig");
    assert!(keys[0]["kid"].as_str().is_some());
    assert!(keys[0]["x"].as_str().is_some());

    shutdown_tx
        .send(())
        .expect("shutdown signal should be delivered");
    handle.await.expect("server task should join");
}
