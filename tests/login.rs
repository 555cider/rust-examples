use std::net::{SocketAddr, TcpListener};

use reqwest::StatusCode;
use sqlx::PgPool;
use tokio::{sync::oneshot, task::JoinHandle};

fn reserve_local_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("ephemeral port should bind");
    let addr = listener.local_addr().expect("listener should have an addr");
    drop(listener);
    addr
}

async fn database_url_for_pool(pool: &PgPool) -> String {
    let database_name: String = sqlx::query_scalar("select current_database()")
        .fetch_one(pool)
        .await
        .expect("current database should be available");

    let base_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for login tests");
    let mut url = url::Url::parse(&base_url).expect("DATABASE_URL should be a valid URL");
    url.set_path(&format!("/{database_name}"));
    url.to_string()
}

async fn spawn_server(database_url: String) -> (SocketAddr, oneshot::Sender<()>, JoinHandle<()>) {
    let bind_addr = reserve_local_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        auth_server::app::run_with_config_and_shutdown(
            auth_server::config::Config {
                bind_addr,
                database_url,
                database_max_connections: 5,
                issuer: format!("http://{bind_addr}"),
                signing_key_path: None,
            },
            async {
                let _ = shutdown_rx.await;
            },
        )
        .await
        .expect("server should run cleanly");
    });

    let client = reqwest::Client::new();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match client
                .get(format!("http://{bind_addr}/healthz"))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => break,
                _ => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .expect("server should become reachable");

    (bind_addr, shutdown_tx, handle)
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn login_creates_session_and_logout_revokes_it(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, sqlx::types::Uuid>(
        r#"
        insert into users (username, email)
        values ($1, $2)
        returning id
        "#,
    )
    .bind("alice")
    .bind("alice@example.com")
    .fetch_one(&pool)
    .await
    .expect("user insert should succeed");

    let password_hash =
        auth_server::identity::password::hash_password("correct-horse-battery-staple")
            .expect("password should hash");

    sqlx::query(
        r#"
        insert into password_credentials (user_id, password_hash)
        values ($1, $2)
        "#,
    )
    .bind(user_id)
    .bind(password_hash)
    .execute(&pool)
    .await
    .expect("password credential insert should succeed");

    let database_url = database_url_for_pool(&pool).await;
    let (bind_addr, shutdown_tx, handle) = spawn_server(database_url).await;

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .expect("client should build");
    let login_response = client
        .post(format!("http://{bind_addr}/login"))
        .json(&serde_json::json!({
            "username": "alice",
            "password": "correct-horse-battery-staple"
        }))
        .send()
        .await
        .expect("login request should succeed");

    assert_eq!(login_response.status(), StatusCode::NO_CONTENT);

    let set_cookie = login_response
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("login should set a session cookie")
        .to_str()
        .expect("cookie header should be text")
        .to_owned();
    assert!(set_cookie.contains("auth_server_session="));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));

    let stored_hash: String = sqlx::query_scalar(
        r#"
        select session_token_hash
        from sessions
        where user_id = $1
          and revoked_at is null
        "#,
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("session row should exist");
    assert!(!set_cookie.contains(&stored_hash));

    let logout_response = client
        .post(format!("http://{bind_addr}/logout"))
        .send()
        .await
        .expect("logout request should succeed");

    assert_eq!(logout_response.status(), StatusCode::NO_CONTENT);

    let revoked_sessions: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from sessions
        where user_id = $1
          and revoked_at is not null
        "#,
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("revoked session count should load");
    assert_eq!(revoked_sessions, 1);

    shutdown_tx
        .send(())
        .expect("shutdown signal should be delivered");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn login_rejects_invalid_password(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, sqlx::types::Uuid>(
        r#"
        insert into users (username, email)
        values ($1, $2)
        returning id
        "#,
    )
    .bind("bob")
    .bind("bob@example.com")
    .fetch_one(&pool)
    .await
    .expect("user insert should succeed");

    let password_hash = auth_server::identity::password::hash_password("expected-password")
        .expect("hashing should work");

    sqlx::query(
        r#"
        insert into password_credentials (user_id, password_hash)
        values ($1, $2)
        "#,
    )
    .bind(user_id)
    .bind(password_hash)
    .execute(&pool)
    .await
    .expect("password credential insert should succeed");

    let database_url = database_url_for_pool(&pool).await;
    let (bind_addr, shutdown_tx, handle) = spawn_server(database_url).await;

    let response = reqwest::Client::new()
        .post(format!("http://{bind_addr}/login"))
        .json(&serde_json::json!({
            "username": "bob",
            "password": "wrong-password"
        }))
        .send()
        .await
        .expect("login request should succeed");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let session_count: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from sessions
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("session count should load");
    assert_eq!(session_count, 0);

    shutdown_tx
        .send(())
        .expect("shutdown signal should be delivered");
    handle.await.expect("server task should join");
}
