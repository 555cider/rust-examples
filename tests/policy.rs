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

    let base_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for policy tests");
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

async fn create_subject_role_binding(
    pool: &PgPool,
    subject_type: &str,
    subject_id: sqlx::types::Uuid,
    role_name: &str,
    permission_name: &str,
) {
    let role_id = sqlx::query_scalar::<_, sqlx::types::Uuid>(
        r#"
        insert into roles (role_name)
        values ($1)
        returning id
        "#,
    )
    .bind(role_name)
    .fetch_one(pool)
    .await
    .expect("role should insert");

    let permission_id = sqlx::query_scalar::<_, sqlx::types::Uuid>(
        r#"
        insert into permissions (permission_name)
        values ($1)
        returning id
        "#,
    )
    .bind(permission_name)
    .fetch_one(pool)
    .await
    .expect("permission should insert");

    sqlx::query(
        r#"
        insert into role_permissions (role_id, permission_id)
        values ($1, $2)
        "#,
    )
    .bind(role_id)
    .bind(permission_id)
    .execute(pool)
    .await
    .expect("role permission should insert");

    sqlx::query(
        r#"
        insert into subject_role_bindings (subject_type, subject_id, role_id)
        values ($1, $2, $3)
        "#,
    )
    .bind(subject_type)
    .bind(subject_id)
    .bind(role_id)
    .execute(pool)
    .await
    .expect("subject role binding should insert");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn policy_evaluate_allows_via_rbac_permission(pool: PgPool) {
    let subject_id = sqlx::types::Uuid::new_v4();
    create_subject_role_binding(
        &pool,
        "user",
        subject_id,
        "document-reader",
        "document:read",
    )
    .await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;

    let response = reqwest::Client::new()
        .post(format!("{issuer}/policy/evaluate"))
        .json(&serde_json::json!({
            "subject": { "type": "user", "id": subject_id },
            "resource": "document",
            "action": "read",
            "context": {}
        }))
        .send()
        .await
        .expect("policy request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("body should parse");
    assert_eq!(body["decision"], "allow");
    assert_eq!(body["reason"], "rbac permission matched");

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn policy_evaluate_denies_when_matching_deny_rule_overrides_rbac(pool: PgPool) {
    let subject_id = sqlx::types::Uuid::new_v4();
    create_subject_role_binding(
        &pool,
        "user",
        subject_id,
        "document-reader",
        "document:read",
    )
    .await;

    sqlx::query(
        r#"
        insert into policy_rules (
            rule_name,
            effect,
            resource_pattern,
            action_pattern,
            condition_expression,
            priority
        )
        values ($1, 'deny', 'document', 'read', $2, 1000)
        "#,
    )
    .bind("deny-sensitive-documents")
    .bind(serde_json::json!({ "sensitivity": "high" }))
    .execute(&pool)
    .await
    .expect("policy rule should insert");

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;

    let response = reqwest::Client::new()
        .post(format!("{issuer}/policy/evaluate"))
        .json(&serde_json::json!({
            "subject": { "type": "user", "id": subject_id },
            "resource": "document",
            "action": "read",
            "context": { "sensitivity": "high" }
        }))
        .send()
        .await
        .expect("policy request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("body should parse");
    assert_eq!(body["decision"], "deny");
    assert_eq!(
        body["matched_rules"],
        serde_json::json!(["deny-sensitive-documents"])
    );

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn policy_evaluate_allows_when_matching_allow_rule_without_rbac(pool: PgPool) {
    let subject_id = sqlx::types::Uuid::new_v4();

    sqlx::query(
        r#"
        insert into policy_rules (
            rule_name,
            effect,
            resource_pattern,
            action_pattern,
            condition_expression,
            priority
        )
        values ($1, 'allow', 'invoice/*', 'approve', $2, 500)
        "#,
    )
    .bind("allow-finance-approvals")
    .bind(serde_json::json!({ "department": "finance" }))
    .execute(&pool)
    .await
    .expect("policy rule should insert");

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;

    let response = reqwest::Client::new()
        .post(format!("{issuer}/policy/evaluate"))
        .json(&serde_json::json!({
            "subject": { "type": "user", "id": subject_id },
            "resource": "invoice/123",
            "action": "approve",
            "context": { "department": "finance" }
        }))
        .send()
        .await
        .expect("policy request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("body should parse");
    assert_eq!(body["decision"], "allow");
    assert_eq!(
        body["matched_rules"],
        serde_json::json!(["allow-finance-approvals"])
    );

    shutdown_tx.send(()).expect("shutdown should succeed");
    handle.await.expect("server task should join");
}
