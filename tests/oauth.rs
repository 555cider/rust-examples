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

    let base_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for oauth tests");
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

async fn insert_confidential_client(
    pool: &PgPool,
    client_id: &str,
    client_secret: &str,
) -> sqlx::types::Uuid {
    let client_secret_hash =
        auth_server::identity::password::hash_password(client_secret).expect("secret should hash");

    sqlx::query_scalar::<_, sqlx::types::Uuid>(
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
        returning id
        "#,
    )
    .bind(client_id)
    .bind(client_secret_hash)
    .bind(format!("{client_id} app"))
    .bind(vec!["api.read".to_owned()])
    .bind(vec!["client_credentials".to_owned()])
    .fetch_one(pool)
    .await
    .expect("confidential client insert should succeed")
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
    state: &str,
) -> url::Url {
    let response = client
        .get(format!(
            "{issuer}/authorize?response_type=code&client_id={client_id}&redirect_uri={redirect_uri}&scope=openid%20offline_access&state={state}&code_challenge={}&code_challenge_method=S256",
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
async fn authorize_redirects_with_code_and_state_for_logged_in_user(pool: PgPool) {
    let redirect_uri = "http://127.0.0.1:9000/callback";
    let client_id = "public-web";
    let code_verifier = "verifier-12345678901234567890";
    let state = "opaque-state";

    let user_id = create_user_with_password(&pool, "carol", "correct-password").await;
    let oauth_client_id = insert_public_client(&pool, client_id, redirect_uri).await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(Policy::none())
        .build()
        .expect("client should build");

    login(&client, &issuer, "carol", "correct-password").await;

    let redirect = authorize_code(
        &client,
        &issuer,
        client_id,
        redirect_uri,
        code_verifier,
        state,
    )
    .await;

    assert_eq!(redirect.scheme(), "http");
    assert_eq!(redirect.host_str(), Some("127.0.0.1"));
    assert_eq!(
        redirect
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.to_string()),
        Some(state.to_owned())
    );
    let code = redirect
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .expect("authorization code should be present");

    let stored_hash: String = sqlx::query_scalar(
        r#"
        select code_hash
        from oauth_authorization_codes
        where client_id = $1
          and user_id = $2
        "#,
    )
    .bind(oauth_client_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("auth code should be stored");
    assert_ne!(stored_hash, code);

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn token_exchanges_authorization_code_with_pkce(pool: PgPool) {
    let redirect_uri = "http://127.0.0.1:9000/callback";
    let client_id = "public-web";
    let code_verifier = "verifier-12345678901234567890";

    let user_id = create_user_with_password(&pool, "dave", "correct-password").await;
    let oauth_client_id = insert_public_client(&pool, client_id, redirect_uri).await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let browser = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(Policy::none())
        .build()
        .expect("browser client should build");

    login(&browser, &issuer, "dave", "correct-password").await;
    let redirect = authorize_code(
        &browser,
        &issuer,
        client_id,
        redirect_uri,
        code_verifier,
        "state-1",
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
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["scope"], "openid offline_access");
    assert!(body["access_token"].as_str().is_some());
    let refresh_token = body["refresh_token"]
        .as_str()
        .expect("refresh token should be returned")
        .to_owned();

    let consumed_at: Option<String> = sqlx::query_scalar(
        r#"
        select consumed_at::text
        from oauth_authorization_codes
        where client_id = $1
          and user_id = $2
        "#,
    )
    .bind(oauth_client_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("auth code should still exist");
    assert!(consumed_at.is_some());

    let stored_refresh_hash: String = sqlx::query_scalar(
        r#"
        select refresh_token_hash
        from oauth_refresh_tokens
        where client_id = $1
          and user_id = $2
          and revoked_at is null
        "#,
    )
    .bind(oauth_client_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("refresh token should be stored");
    assert_ne!(stored_refresh_hash, refresh_token);

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn token_issues_client_credentials_access_token(pool: PgPool) {
    insert_confidential_client(&pool, "service-client", "secret-value").await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;

    let token_response = reqwest::Client::new()
        .post(format!("{issuer}/token"))
        .basic_auth("service-client", Some("secret-value"))
        .form(&[("grant_type", "client_credentials"), ("scope", "api.read")])
        .send()
        .await
        .expect("token request should succeed");

    assert_eq!(token_response.status(), StatusCode::OK);

    let body: serde_json::Value = token_response.json().await.expect("body should be json");
    assert_eq!(body["token_type"], "Bearer");
    assert!(body.get("refresh_token").is_none());

    let access_token = body["access_token"]
        .as_str()
        .expect("access token should be returned");
    let segments: Vec<&str> = access_token.split('.').collect();
    let claims: serde_json::Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(segments[1])
            .expect("claims should decode"),
    )
    .expect("claims should parse");
    assert_eq!(claims["sub"], "service-client");
    assert_eq!(claims["client_id"], "service-client");
    assert_eq!(claims["azp"], "service-client");
    assert_eq!(claims["scope"], "api.read");

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn refresh_token_rotation_rejects_token_reuse(pool: PgPool) {
    let redirect_uri = "http://127.0.0.1:9000/callback";
    let client_id = "public-web";
    let code_verifier = "verifier-12345678901234567890";

    create_user_with_password(&pool, "erin", "correct-password").await;
    insert_public_client(&pool, client_id, redirect_uri).await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let browser = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(Policy::none())
        .build()
        .expect("browser client should build");

    login(&browser, &issuer, "erin", "correct-password").await;
    let redirect = authorize_code(
        &browser,
        &issuer,
        client_id,
        redirect_uri,
        code_verifier,
        "state-2",
    )
    .await;
    let code = redirect
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .expect("authorization code should be present");

    let token_client = reqwest::Client::new();
    let initial_token_response = token_client
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
        .expect("initial token request should succeed");
    let initial_body: serde_json::Value = initial_token_response
        .json()
        .await
        .expect("token body should parse");
    let refresh_token = initial_body["refresh_token"]
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
        .expect("refresh token request should succeed");
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
        .expect("reused refresh token request should succeed");
    assert_eq!(reused_response.status(), StatusCode::BAD_REQUEST);
    let reused_body: serde_json::Value = reused_response
        .json()
        .await
        .expect("error body should parse");
    assert_eq!(reused_body["error"], "invalid_grant");

    let reused_at: Option<String> = sqlx::query_scalar(
        r#"
        select reused_at::text
        from oauth_refresh_tokens
        where refresh_token_hash = encode(digest($1, 'sha256'), 'hex')
        "#,
    )
    .bind(refresh_token)
    .fetch_one(&pool)
    .await
    .expect("stored refresh token should load");
    assert!(reused_at.is_some());

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}
