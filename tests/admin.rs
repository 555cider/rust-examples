use std::net::{SocketAddr, TcpListener};

use reqwest::StatusCode;
use serde_json::Value;
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

    let base_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for admin tests");
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

async fn grant_admin_permission(
    pool: &PgPool,
    user_id: sqlx::types::Uuid,
    suffix: &str,
) -> sqlx::types::Uuid {
    let role_id = sqlx::query_scalar::<_, sqlx::types::Uuid>(
        r#"
        insert into roles (role_name, description)
        values ($1, $2)
        returning id
        "#,
    )
    .bind(format!("admin-role-{suffix}"))
    .bind("Admin test role")
    .fetch_one(pool)
    .await
    .expect("role insert should succeed");

    let permission_id = sqlx::query_scalar::<_, sqlx::types::Uuid>(
        r#"
        insert into permissions (permission_name, description)
        values ('admin:manage', 'Admin management')
        on conflict (permission_name)
        do update set description = excluded.description
        returning id
        "#,
    )
    .fetch_one(pool)
    .await
    .expect("permission insert should succeed");

    sqlx::query(
        r#"
        insert into role_permissions (role_id, permission_id)
        values ($1, $2)
        on conflict do nothing
        "#,
    )
    .bind(role_id)
    .bind(permission_id)
    .execute(pool)
    .await
    .expect("role permission insert should succeed");

    sqlx::query(
        r#"
        insert into subject_role_bindings (subject_type, subject_id, role_id)
        values ('user', $1, $2)
        "#,
    )
    .bind(user_id)
    .bind(role_id)
    .execute(pool)
    .await
    .expect("subject role binding insert should succeed");

    role_id
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

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn admin_routes_require_admin_permission(pool: PgPool) {
    create_user_with_password(&pool, "regular-admin-check", "correct-password").await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .expect("client should build");

    login(&client, &issuer, "regular-admin-check", "correct-password").await;

    let response = client
        .get(format!("{issuer}/admin/users"))
        .send()
        .await
        .expect("admin request should succeed");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    shutdown_tx
        .send(())
        .expect("shutdown signal should be delivered");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn admin_user_crud_records_audit_logs(pool: PgPool) {
    let admin_user_id = create_user_with_password(&pool, "admin-user", "correct-password").await;
    grant_admin_permission(&pool, admin_user_id, "user-crud").await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .expect("client should build");

    login(&client, &issuer, "admin-user", "correct-password").await;

    let create_response = client
        .post(format!("{issuer}/admin/users"))
        .json(&serde_json::json!({
            "username": "managed-user",
            "email": "managed-user@example.com",
            "display_name": "Managed User",
            "password": "temporary-password"
        }))
        .send()
        .await
        .expect("create user request should succeed");

    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created_user: Value = create_response
        .json()
        .await
        .expect("create user response should be json");
    let created_user_id = created_user["id"]
        .as_str()
        .expect("created user should have an id")
        .to_owned();

    let list_response = client
        .get(format!("{issuer}/admin/users"))
        .send()
        .await
        .expect("list users request should succeed");

    assert_eq!(list_response.status(), StatusCode::OK);
    let users: Vec<Value> = list_response
        .json()
        .await
        .expect("users response should be json");
    assert!(users.iter().any(|user| {
        user["id"].as_str() == Some(created_user_id.as_str())
            && user["username"].as_str() == Some("managed-user")
    }));

    let update_response = client
        .patch(format!("{issuer}/admin/users/{created_user_id}"))
        .json(&serde_json::json!({
            "display_name": "Managed User Updated",
            "is_active": false
        }))
        .send()
        .await
        .expect("update user request should succeed");

    assert_eq!(update_response.status(), StatusCode::OK);
    let updated_user: Value = update_response
        .json()
        .await
        .expect("update response should be json");
    assert_eq!(
        updated_user["display_name"].as_str(),
        Some("Managed User Updated")
    );
    assert_eq!(updated_user["is_active"].as_bool(), Some(false));

    let delete_response = client
        .delete(format!("{issuer}/admin/users/{created_user_id}"))
        .send()
        .await
        .expect("delete user request should succeed");

    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let event_types: Vec<String> = sqlx::query_scalar(
        r#"
        select event_type
        from audit_logs
        where actor_type = 'user'
          and actor_id = $1
          and target_type = 'user'
        order by occurred_at asc
        "#,
    )
    .bind(admin_user_id)
    .fetch_all(&pool)
    .await
    .expect("audit events should load");

    assert_eq!(
        event_types,
        vec![
            "admin.user.created".to_owned(),
            "admin.user.updated".to_owned(),
            "admin.user.deleted".to_owned()
        ]
    );

    let managed_user_is_active: bool = sqlx::query_scalar(
        r#"
        select is_active
        from users
        where id = $1
        "#,
    )
    .bind(uuid::Uuid::parse_str(&created_user_id).expect("created user id should be a valid uuid"))
    .fetch_one(&pool)
    .await
    .expect("managed user should still exist");
    assert!(!managed_user_is_active);

    shutdown_tx
        .send(())
        .expect("shutdown signal should be delivered");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn admin_client_crud_records_audit_logs(pool: PgPool) {
    let admin_user_id =
        create_user_with_password(&pool, "admin-client-user", "correct-password").await;
    grant_admin_permission(&pool, admin_user_id, "client-crud").await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .expect("client should build");

    login(&client, &issuer, "admin-client-user", "correct-password").await;

    let create_response = client
        .post(format!("{issuer}/admin/clients"))
        .json(&serde_json::json!({
            "client_id": "cli-app",
            "client_secret": "super-secret",
            "client_name": "CLI App",
            "redirect_uris": ["http://127.0.0.1:9000/callback"],
            "scopes": ["openid", "profile"],
            "grant_types": ["authorization_code", "refresh_token"],
            "auth_method": "client_secret_basic",
            "is_confidential": true
        }))
        .send()
        .await
        .expect("create client request should succeed");

    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created_client: Value = create_response
        .json()
        .await
        .expect("client create response should be json");
    let created_client_id = created_client["id"]
        .as_str()
        .expect("created client should have an id")
        .to_owned();

    let list_response = client
        .get(format!("{issuer}/admin/clients"))
        .send()
        .await
        .expect("list clients request should succeed");

    assert_eq!(list_response.status(), StatusCode::OK);
    let clients: Vec<Value> = list_response
        .json()
        .await
        .expect("clients response should be json");
    assert!(clients.iter().any(|oauth_client| {
        oauth_client["id"].as_str() == Some(created_client_id.as_str())
            && oauth_client["client_id"].as_str() == Some("cli-app")
    }));

    let update_response = client
        .patch(format!("{issuer}/admin/clients/{created_client_id}"))
        .json(&serde_json::json!({
            "client_name": "CLI App Updated",
            "is_active": false
        }))
        .send()
        .await
        .expect("update client request should succeed");

    assert_eq!(update_response.status(), StatusCode::OK);
    let updated_client: Value = update_response
        .json()
        .await
        .expect("client update response should be json");
    assert_eq!(
        updated_client["client_name"].as_str(),
        Some("CLI App Updated")
    );
    assert_eq!(updated_client["is_active"].as_bool(), Some(false));

    let delete_response = client
        .delete(format!("{issuer}/admin/clients/{created_client_id}"))
        .send()
        .await
        .expect("delete client request should succeed");

    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let event_types: Vec<String> = sqlx::query_scalar(
        r#"
        select event_type
        from audit_logs
        where actor_type = 'user'
          and actor_id = $1
          and target_type = 'oauth_client'
        order by occurred_at asc
        "#,
    )
    .bind(admin_user_id)
    .fetch_all(&pool)
    .await
    .expect("client audit events should load");

    assert_eq!(
        event_types,
        vec![
            "admin.client.created".to_owned(),
            "admin.client.updated".to_owned(),
            "admin.client.deleted".to_owned()
        ]
    );

    let client_is_active: bool = sqlx::query_scalar(
        r#"
        select is_active
        from oauth_clients
        where id = $1
        "#,
    )
    .bind(
        uuid::Uuid::parse_str(&created_client_id)
            .expect("created client id should be a valid uuid"),
    )
    .fetch_one(&pool)
    .await
    .expect("managed client should still exist");
    assert!(!client_is_active);

    shutdown_tx
        .send(())
        .expect("shutdown signal should be delivered");
    handle.await.expect("server task should join");
}

#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn admin_role_permission_and_policy_crud_record_audit_logs(pool: PgPool) {
    let admin_user_id =
        create_user_with_password(&pool, "admin-policy-user", "correct-password").await;
    grant_admin_permission(&pool, admin_user_id, "policy-crud").await;

    let database_url = database_url_for_pool(&pool).await;
    let (issuer, shutdown_tx, handle) = spawn_server(database_url).await;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .expect("client should build");

    login(&client, &issuer, "admin-policy-user", "correct-password").await;

    let role_create_response = client
        .post(format!("{issuer}/admin/roles"))
        .json(&serde_json::json!({
            "role_name": "auditor",
            "description": "Read-only auditor",
            "is_system": false
        }))
        .send()
        .await
        .expect("create role request should succeed");
    assert_eq!(role_create_response.status(), StatusCode::CREATED);
    let role_id = role_create_response
        .json::<Value>()
        .await
        .expect("role response should be json")["id"]
        .as_str()
        .expect("role should have an id")
        .to_owned();

    let role_update_response = client
        .patch(format!("{issuer}/admin/roles/{role_id}"))
        .json(&serde_json::json!({
            "description": "Updated auditor role"
        }))
        .send()
        .await
        .expect("update role request should succeed");
    assert_eq!(role_update_response.status(), StatusCode::OK);

    let permission_create_response = client
        .post(format!("{issuer}/admin/permissions"))
        .json(&serde_json::json!({
            "permission_name": "reports:read",
            "description": "Read reports"
        }))
        .send()
        .await
        .expect("create permission request should succeed");
    assert_eq!(permission_create_response.status(), StatusCode::CREATED);
    let permission_id = permission_create_response
        .json::<Value>()
        .await
        .expect("permission response should be json")["id"]
        .as_str()
        .expect("permission should have an id")
        .to_owned();

    let permission_update_response = client
        .patch(format!("{issuer}/admin/permissions/{permission_id}"))
        .json(&serde_json::json!({
            "description": "Read reporting data"
        }))
        .send()
        .await
        .expect("update permission request should succeed");
    assert_eq!(permission_update_response.status(), StatusCode::OK);

    let policy_create_response = client
        .post(format!("{issuer}/admin/policies"))
        .json(&serde_json::json!({
            "rule_name": "reports-allow",
            "effect": "allow",
            "resource_pattern": "report:*",
            "action_pattern": "read",
            "condition_expression": {
                "department": "finance"
            },
            "priority": 300,
            "description": "Finance can read reports"
        }))
        .send()
        .await
        .expect("create policy request should succeed");
    assert_eq!(policy_create_response.status(), StatusCode::CREATED);
    let policy_id = policy_create_response
        .json::<Value>()
        .await
        .expect("policy response should be json")["id"]
        .as_str()
        .expect("policy should have an id")
        .to_owned();

    let policy_update_response = client
        .patch(format!("{issuer}/admin/policies/{policy_id}"))
        .json(&serde_json::json!({
            "priority": 400,
            "is_active": false
        }))
        .send()
        .await
        .expect("update policy request should succeed");
    assert_eq!(policy_update_response.status(), StatusCode::OK);

    let list_roles = client
        .get(format!("{issuer}/admin/roles"))
        .send()
        .await
        .expect("list roles request should succeed");
    assert_eq!(list_roles.status(), StatusCode::OK);

    let list_permissions = client
        .get(format!("{issuer}/admin/permissions"))
        .send()
        .await
        .expect("list permissions request should succeed");
    assert_eq!(list_permissions.status(), StatusCode::OK);

    let list_policies = client
        .get(format!("{issuer}/admin/policies"))
        .send()
        .await
        .expect("list policies request should succeed");
    assert_eq!(list_policies.status(), StatusCode::OK);

    let delete_role_response = client
        .delete(format!("{issuer}/admin/roles/{role_id}"))
        .send()
        .await
        .expect("delete role request should succeed");
    assert_eq!(delete_role_response.status(), StatusCode::NO_CONTENT);

    let delete_permission_response = client
        .delete(format!("{issuer}/admin/permissions/{permission_id}"))
        .send()
        .await
        .expect("delete permission request should succeed");
    assert_eq!(delete_permission_response.status(), StatusCode::NO_CONTENT);

    let delete_policy_response = client
        .delete(format!("{issuer}/admin/policies/{policy_id}"))
        .send()
        .await
        .expect("delete policy request should succeed");
    assert_eq!(delete_policy_response.status(), StatusCode::NO_CONTENT);

    let event_types: Vec<String> = sqlx::query_scalar(
        r#"
        select event_type
        from audit_logs
        where actor_type = 'user'
          and actor_id = $1
          and target_type in ('role', 'permission', 'policy_rule')
        order by occurred_at asc
        "#,
    )
    .bind(admin_user_id)
    .fetch_all(&pool)
    .await
    .expect("admin audit events should load");

    assert_eq!(
        event_types,
        vec![
            "admin.role.created".to_owned(),
            "admin.role.updated".to_owned(),
            "admin.permission.created".to_owned(),
            "admin.permission.updated".to_owned(),
            "admin.policy.created".to_owned(),
            "admin.policy.updated".to_owned(),
            "admin.role.deleted".to_owned(),
            "admin.permission.deleted".to_owned(),
            "admin.policy.deleted".to_owned()
        ]
    );

    shutdown_tx
        .send(())
        .expect("shutdown signal should be delivered");
    handle.await.expect("server task should join");
}
