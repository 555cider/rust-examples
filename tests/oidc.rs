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

    let base_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for oidc tests");
    let mut url = url::Url::parse(&base_url).expect("DATABASE_URL should be a valid URL");
    url.set_path(&format!("/{database_name}"));
    url.to_string()
}

async fn spawn_server(database_url: String) -> (String, oneshot::Sender<()>, JoinHandle<()>) {
    let bind_addr = reserve_local_addr();
    let issuer = format!("http://{bind_addr}");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        auth_server::app::run_with_config_and_shutdown(
            auth_server::config::Config {
                bind_addr,
                database_url,
                database_max_connections: 5,
                issuer: issuer.clone(),
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

    (format!("http://{bind_addr}"), shutdown_tx, handle)
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
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
        "email".to_owned(),
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

async fn authorize_code(
    client: &reqwest::Client,
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
    nonce: &str,
) -> url::Url {
    let response = client
        .get(format!(
            "{issuer}/authorize?response_type=code&client_id={client_id}&redirect_uri={redirect_uri}&scope=openid%20profile%20email%20offline_access&state=state-oidc&nonce={nonce}&code_challenge={}&code_challenge_method=S256",
            pkce_challenge(code_verifier)
        ))
        .send()
        .await
        .expect("authorize request should succeed");

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    url::Url::parse(
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .expect("authorize should redirect")
            .to_str()
            .expect("location should be text"),
    )
    .expect("redirect location should be valid")
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn authorization_code_exchange_returns_id_token_for_openid_scope(pool: PgPool) {
    let redirect_uri = "http://127.0.0.1:9000/callback";
    let client_id = "oidc-web";
    let code_verifier = "oidc-verifier-12345678901234567890";
    let nonce = "nonce-123";

    let user_id = create_user_with_password(&pool, "frank", "correct-password").await;
    insert_public_client(&pool, client_id, redirect_uri).await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let browser = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(Policy::none())
        .build()
        .expect("browser client should build");

    login(&browser, &issuer, "frank", "correct-password").await;
    let redirect = authorize_code(
        &browser,
        &issuer,
        client_id,
        redirect_uri,
        code_verifier,
        nonce,
    )
    .await;
    let code = redirect
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .expect("authorization code should be present");

    let token_response = reqwest::Client::new()
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
        .expect("token request should succeed");

    assert_eq!(token_response.status(), StatusCode::OK);

    let body: serde_json::Value = token_response.json().await.expect("body should be json");
    let id_token = body["id_token"]
        .as_str()
        .expect("id token should be present");
    let claims: serde_json::Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(
                id_token
                    .split('.')
                    .nth(1)
                    .expect("claims segment should exist"),
            )
            .expect("claims should decode"),
    )
    .expect("claims should parse");
    assert_eq!(claims["iss"], issuer);
    assert_eq!(claims["sub"], user_id.to_string());
    assert_eq!(claims["aud"], client_id);
    assert_eq!(claims["nonce"], nonce);

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn userinfo_returns_subject_profile_and_email(pool: PgPool) {
    let redirect_uri = "http://127.0.0.1:9000/callback";
    let client_id = "oidc-web";
    let code_verifier = "oidc-verifier-12345678901234567890";

    let user_id = create_user_with_password(&pool, "grace", "correct-password").await;
    insert_public_client(&pool, client_id, redirect_uri).await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let browser = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(Policy::none())
        .build()
        .expect("browser client should build");

    login(&browser, &issuer, "grace", "correct-password").await;
    let redirect = authorize_code(
        &browser,
        &issuer,
        client_id,
        redirect_uri,
        code_verifier,
        "nonce-456",
    )
    .await;
    let code = redirect
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .expect("authorization code should be present");

    let token_body: serde_json::Value = reqwest::Client::new()
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
        .expect("token request should succeed")
        .json()
        .await
        .expect("token body should parse");

    let access_token = token_body["access_token"]
        .as_str()
        .expect("access token should be present");

    let userinfo_response = reqwest::Client::new()
        .get(format!("{issuer}/userinfo"))
        .bearer_auth(access_token)
        .send()
        .await
        .expect("userinfo request should succeed");

    assert_eq!(userinfo_response.status(), StatusCode::OK);

    let userinfo: serde_json::Value = userinfo_response
        .json()
        .await
        .expect("userinfo should be json");
    assert_eq!(userinfo["sub"], user_id.to_string());
    assert_eq!(userinfo["preferred_username"], "grace");
    assert_eq!(userinfo["email"], "grace@example.com");

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}
