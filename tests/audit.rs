use std::net::{SocketAddr, TcpListener};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{StatusCode, redirect::Policy};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, types::Json};
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

    let base_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for audit tests");
    let mut url = url::Url::parse(&base_url).expect("DATABASE_URL should be a valid URL");
    url.set_path(&format!("/{database_name}"));
    url.to_string()
}

async fn spawn_server(database_url: String) -> (String, oneshot::Sender<()>, JoinHandle<()>) {
    let bind_addr = reserve_local_addr();
    let issuer = format!("http://{bind_addr}");
    let task_issuer = issuer.clone();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        auth_server::app::run_with_config_and_shutdown(
            auth_server::config::Config {
                bind_addr,
                database_url,
                database_max_connections: 5,
                issuer: task_issuer,
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

async fn create_user_with_password(
    pool: &PgPool,
    username: &str,
    password: &str,
) -> sqlx::types::Uuid {
    let user_id = sqlx::query_scalar::<_, sqlx::types::Uuid>(
        r#"
        insert into users (username, email)
        values ($1, $2)
        returning id
        "#,
    )
    .bind(username)
    .bind(format!("{username}@example.com"))
    .fetch_one(pool)
    .await
    .expect("user insert should succeed");

    let password_hash =
        auth_server::identity::password::hash_password(password).expect("password should hash");
    sqlx::query(
        r#"
        insert into password_credentials (user_id, password_hash)
        values ($1, $2)
        "#,
    )
    .bind(user_id)
    .bind(password_hash)
    .execute(pool)
    .await
    .expect("password credential insert should succeed");

    user_id
}

async fn insert_public_client(
    pool: &PgPool,
    client_id: &str,
    redirect_uri: &str,
) -> sqlx::types::Uuid {
    sqlx::query_scalar::<_, sqlx::types::Uuid>(
        r#"
        insert into oauth_clients (
            client_id,
            client_name,
            redirect_uris,
            scopes,
            grant_types,
            auth_method,
            is_confidential
        )
        values ($1, $2, $3, $4, $5, 'none', false)
        returning id
        "#,
    )
    .bind(client_id)
    .bind(format!("{client_id} app"))
    .bind(Json(vec![redirect_uri.to_owned()]))
    .bind(vec![
        "openid".to_owned(),
        "profile".to_owned(),
        "offline_access".to_owned(),
    ])
    .bind(vec![
        "authorization_code".to_owned(),
        "refresh_token".to_owned(),
    ])
    .fetch_one(pool)
    .await
    .expect("public client insert should succeed")
}

async fn insert_confidential_client(pool: &PgPool, client_id: &str, client_secret: &str) {
    let client_secret_hash =
        auth_server::identity::password::hash_password(client_secret).expect("secret should hash");

    sqlx::query(
        r#"
        insert into oauth_clients (
            client_id,
            client_secret_hash,
            client_name,
            redirect_uris,
            scopes,
            grant_types,
            auth_method,
            is_confidential
        )
        values ($1, $2, $3, '[]'::jsonb, $4, $5, 'client_secret_basic', true)
        "#,
    )
    .bind(client_id)
    .bind(client_secret_hash)
    .bind(format!("{client_id} app"))
    .bind(vec!["api.read".to_owned()])
    .bind(vec!["client_credentials".to_owned()])
    .execute(pool)
    .await
    .expect("confidential client insert should succeed");
}

async fn login(client: &reqwest::Client, issuer: &str, username: &str, password: &str) {
    let response = client
        .post(format!("{issuer}/login"))
        .json(&serde_json::json!({
            "username": username,
            "password": password
        }))
        .send()
        .await
        .expect("login request should succeed");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

async fn authorize_code(
    client: &reqwest::Client,
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> String {
    let response = client
        .get(format!(
            "{issuer}/authorize?response_type=code&client_id={client_id}&redirect_uri={redirect_uri}&scope=openid%20offline_access&state=opaque-state&code_challenge={}&code_challenge_method=S256",
            pkce_challenge(code_verifier)
        ))
        .send()
        .await
        .expect("authorize request should succeed");

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let redirect = url::Url::parse(
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .expect("authorize should redirect")
            .to_str()
            .expect("location should be text"),
    )
    .expect("redirect should be valid");

    redirect
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .expect("authorization code should be present")
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn auth_login_and_logout_events_are_audited(pool: PgPool) {
    create_user_with_password(&pool, "audit-alice", "correct-password").await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .expect("client should build");

    let failed_login = client
        .post(format!("{issuer}/login"))
        .json(&serde_json::json!({
            "username": "audit-alice",
            "password": "wrong-password"
        }))
        .send()
        .await
        .expect("failed login request should succeed");
    assert_eq!(failed_login.status(), StatusCode::UNAUTHORIZED);

    login(&client, &issuer, "audit-alice", "correct-password").await;

    let logout_response = client
        .post(format!("{issuer}/logout"))
        .send()
        .await
        .expect("logout request should succeed");
    assert_eq!(logout_response.status(), StatusCode::NO_CONTENT);

    let event_types: Vec<String> = sqlx::query_scalar(
        r#"
        select event_type
        from audit_logs
        where event_type like 'auth.%'
        order by occurred_at asc
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("auth audit events should load");

    assert_eq!(
        event_types,
        vec![
            "auth.login.failed".to_owned(),
            "auth.login.succeeded".to_owned(),
            "auth.logout.succeeded".to_owned()
        ]
    );

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn oauth_events_are_audited_for_code_client_credentials_and_refresh_reuse(pool: PgPool) {
    let redirect_uri = "http://127.0.0.1:9000/callback";
    let client_id = "audit-public";
    let confidential_client_id = "audit-service";
    let code_verifier = "verifier-12345678901234567890";

    create_user_with_password(&pool, "audit-bob", "correct-password").await;
    insert_public_client(&pool, client_id, redirect_uri).await;
    insert_confidential_client(&pool, confidential_client_id, "secret-value").await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let browser = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(Policy::none())
        .build()
        .expect("browser client should build");

    login(&browser, &issuer, "audit-bob", "correct-password").await;
    let code = authorize_code(&browser, &issuer, client_id, redirect_uri, code_verifier).await;

    let token_client = reqwest::Client::new();
    let auth_code_token = token_client
        .post(format!("{issuer}/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ])
        .send()
        .await
        .expect("authorization code exchange should succeed");
    assert_eq!(auth_code_token.status(), StatusCode::OK);
    let auth_code_body: serde_json::Value = auth_code_token
        .json()
        .await
        .expect("authorization code body should parse");
    let refresh_token = auth_code_body["refresh_token"]
        .as_str()
        .expect("refresh token should be returned")
        .to_owned();

    let rotated_response = token_client
        .post(format!("{issuer}/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token.as_str()),
        ])
        .send()
        .await
        .expect("refresh rotation request should succeed");
    assert_eq!(rotated_response.status(), StatusCode::OK);

    let reused_response = token_client
        .post(format!("{issuer}/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token.as_str()),
        ])
        .send()
        .await
        .expect("refresh reuse request should succeed");
    assert_eq!(reused_response.status(), StatusCode::BAD_REQUEST);

    let client_credentials_response = token_client
        .post(format!("{issuer}/token"))
        .basic_auth(confidential_client_id, Some("secret-value"))
        .form(&[("grant_type", "client_credentials"), ("scope", "api.read")])
        .send()
        .await
        .expect("client credentials request should succeed");
    assert_eq!(client_credentials_response.status(), StatusCode::OK);

    let event_types: Vec<String> = sqlx::query_scalar(
        r#"
        select event_type
        from audit_logs
        where event_type like 'oauth.%'
        order by occurred_at asc
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("oauth audit events should load");

    assert_eq!(
        event_types,
        vec![
            "oauth.authorization_code.issued".to_owned(),
            "oauth.token.authorization_code_issued".to_owned(),
            "oauth.token.refresh_rotated".to_owned(),
            "oauth.token.refresh_reuse_detected".to_owned(),
            "oauth.token.client_credentials_issued".to_owned()
        ]
    );

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}
