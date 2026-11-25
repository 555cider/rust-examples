use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{
    FromRow, PgPool,
    types::{Json, Uuid},
};

use crate::{
    audit::{self, AuditEvent},
    error::Result,
    identity::password,
};

pub const ADMIN_PERMISSION: &str = "admin:manage";

#[derive(Debug, Serialize, FromRow)]
pub struct AdminUser {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserInput {
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserInput {
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub password: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct AdminOAuthClient {
    pub id: Uuid,
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub scopes: Vec<String>,
    pub grant_types: Vec<String>,
    pub auth_method: String,
    pub is_confidential: bool,
    pub is_active: bool,
}

#[derive(Debug, FromRow)]
struct AdminOAuthClientRow {
    id: Uuid,
    client_id: String,
    client_name: String,
    redirect_uris: Json<Vec<String>>,
    scopes: Vec<String>,
    grant_types: Vec<String>,
    auth_method: String,
    is_confidential: bool,
    is_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateClientInput {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub scopes: Vec<String>,
    pub grant_types: Vec<String>,
    pub auth_method: String,
    pub is_confidential: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateClientInput {
    pub client_name: Option<String>,
    pub redirect_uris: Option<Vec<String>>,
    pub scopes: Option<Vec<String>>,
    pub grant_types: Option<Vec<String>>,
    pub auth_method: Option<String>,
    pub client_secret: Option<String>,
    pub is_confidential: Option<bool>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct AdminRole {
    pub id: Uuid,
    pub role_name: String,
    pub description: Option<String>,
    pub is_system: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoleInput {
    pub role_name: String,
    pub description: Option<String>,
    pub is_system: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleInput {
    pub role_name: Option<String>,
    pub description: Option<String>,
    pub is_system: Option<bool>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct AdminPermission {
    pub id: Uuid,
    pub permission_name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePermissionInput {
    pub permission_name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePermissionInput {
    pub permission_name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AdminPolicyRule {
    pub id: Uuid,
    pub rule_name: String,
    pub effect: String,
    pub resource_pattern: String,
    pub action_pattern: String,
    pub condition_expression: serde_json::Value,
    pub priority: i32,
    pub is_active: bool,
    pub description: Option<String>,
}

#[derive(Debug, FromRow)]
struct AdminPolicyRuleRow {
    id: Uuid,
    rule_name: String,
    effect: String,
    resource_pattern: String,
    action_pattern: String,
    condition_expression: Json<serde_json::Value>,
    priority: i32,
    is_active: bool,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePolicyInput {
    pub rule_name: String,
    pub effect: String,
    pub resource_pattern: String,
    pub action_pattern: String,
    pub condition_expression: serde_json::Value,
    pub priority: i32,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePolicyInput {
    pub rule_name: Option<String>,
    pub effect: Option<String>,
    pub resource_pattern: Option<String>,
    pub action_pattern: Option<String>,
    pub condition_expression: Option<serde_json::Value>,
    pub priority: Option<i32>,
    pub is_active: Option<bool>,
    pub description: Option<String>,
}

pub async fn user_has_permission(
    pool: &PgPool,
    user_id: Uuid,
    permission_name: &str,
) -> Result<bool> {
    let has_permission = sqlx::query_scalar::<_, bool>(
        r#"
        select exists (
            select 1
            from subject_role_bindings
            inner join role_permissions on role_permissions.role_id = subject_role_bindings.role_id
            inner join permissions on permissions.id = role_permissions.permission_id
            where subject_role_bindings.subject_type = 'user'
              and subject_role_bindings.subject_id = $1
              and permissions.permission_name = $2
        )
        "#,
    )
    .bind(user_id)
    .bind(permission_name)
    .fetch_one(pool)
    .await?;

    Ok(has_permission)
}

pub async fn list_users(pool: &PgPool) -> Result<Vec<AdminUser>> {
    let users = sqlx::query_as(
        r#"
        select id, username, email, display_name, is_active
        from users
        order by created_at asc
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(users)
}

pub async fn create_user(
    pool: &PgPool,
    actor_id: Uuid,
    input: CreateUserInput,
) -> Result<AdminUser> {
    let mut transaction = pool.begin().await?;
    let password_hash = password::hash_password(&input.password)?;

    let user = sqlx::query_as::<_, AdminUser>(
        r#"
        insert into users (username, email, display_name)
        values ($1, $2, $3)
        returning id, username, email, display_name, is_active
        "#,
    )
    .bind(input.username.trim())
    .bind(input.email.trim())
    .bind(input.display_name.as_deref().map(str::trim))
    .fetch_one(&mut *transaction)
    .await?;

    sqlx::query(
        r#"
        insert into password_credentials (user_id, password_hash)
        values ($1, $2)
        "#,
    )
    .bind(user.id)
    .bind(password_hash)
    .execute(&mut *transaction)
    .await?;

    audit::record(
        &mut *transaction,
        AuditEvent {
            actor_type: "user",
            actor_id: Some(actor_id),
            event_type: "admin.user.created",
            target_type: Some("user"),
            target_id: Some(user.id),
            metadata: json!({
                "username": user.username,
                "email": user.email
            }),
        },
    )
    .await?;

    transaction.commit().await?;

    Ok(user)
}

pub async fn update_user(
    pool: &PgPool,
    actor_id: Uuid,
    user_id: Uuid,
    input: UpdateUserInput,
) -> Result<Option<AdminUser>> {
    let mut transaction = pool.begin().await?;

    if let Some(password_value) = input.password.as_deref() {
        let password_hash = password::hash_password(password_value)?;
        sqlx::query(
            r#"
            insert into password_credentials (user_id, password_hash, password_updated_at)
            values ($1, $2, now())
            on conflict (user_id)
            do update set
                password_hash = excluded.password_hash,
                password_updated_at = now()
            "#,
        )
        .bind(user_id)
        .bind(password_hash)
        .execute(&mut *transaction)
        .await?;
    }

    let user = sqlx::query_as::<_, AdminUser>(
        r#"
        update users
        set
            username = coalesce($2, username),
            email = coalesce($3, email),
            display_name = coalesce($4, display_name),
            is_active = coalesce($5, is_active),
            updated_at = now()
        where id = $1
        returning id, username, email, display_name, is_active
        "#,
    )
    .bind(user_id)
    .bind(input.username.as_deref().map(str::trim))
    .bind(input.email.as_deref().map(str::trim))
    .bind(input.display_name.as_deref().map(str::trim))
    .bind(input.is_active)
    .fetch_optional(&mut *transaction)
    .await?;

    if let Some(user) = user {
        audit::record(
            &mut *transaction,
            AuditEvent {
                actor_type: "user",
                actor_id: Some(actor_id),
                event_type: "admin.user.updated",
                target_type: Some("user"),
                target_id: Some(user.id),
                metadata: json!({
                    "username": user.username,
                    "email": user.email,
                    "is_active": user.is_active
                }),
            },
        )
        .await?;

        transaction.commit().await?;
        Ok(Some(user))
    } else {
        transaction.rollback().await?;
        Ok(None)
    }
}

pub async fn deactivate_user(pool: &PgPool, actor_id: Uuid, user_id: Uuid) -> Result<bool> {
    let mut transaction = pool.begin().await?;

    let updated_rows = sqlx::query(
        r#"
        update users
        set is_active = false, updated_at = now()
        where id = $1
        "#,
    )
    .bind(user_id)
    .execute(&mut *transaction)
    .await?
    .rows_affected();

    if updated_rows == 0 {
        transaction.rollback().await?;
        return Ok(false);
    }

    audit::record(
        &mut *transaction,
        AuditEvent {
            actor_type: "user",
            actor_id: Some(actor_id),
            event_type: "admin.user.deleted",
            target_type: Some("user"),
            target_id: Some(user_id),
            metadata: json!({}),
        },
    )
    .await?;

    transaction.commit().await?;

    Ok(true)
}

pub async fn list_clients(pool: &PgPool) -> Result<Vec<AdminOAuthClient>> {
    let rows = sqlx::query_as::<_, AdminOAuthClientRow>(
        r#"
        select
            id,
            client_id,
            client_name,
            redirect_uris,
            scopes,
            grant_types,
            auth_method,
            is_confidential,
            is_active
        from oauth_clients
        order by created_at asc
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(AdminOAuthClient::from).collect())
}

pub async fn create_client(
    pool: &PgPool,
    actor_id: Uuid,
    input: CreateClientInput,
) -> Result<AdminOAuthClient> {
    let mut transaction = pool.begin().await?;
    let client_secret_hash = input
        .client_secret
        .as_deref()
        .map(password::hash_password)
        .transpose()?;

    let row = sqlx::query_as::<_, AdminOAuthClientRow>(
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
        values ($1, $2, $3, $4, $5, $6, $7, $8)
        returning
            id,
            client_id,
            client_name,
            redirect_uris,
            scopes,
            grant_types,
            auth_method,
            is_confidential,
            is_active
        "#,
    )
    .bind(input.client_id.trim())
    .bind(client_secret_hash)
    .bind(input.client_name.trim())
    .bind(Json(input.redirect_uris))
    .bind(input.scopes)
    .bind(input.grant_types)
    .bind(input.auth_method.trim())
    .bind(input.is_confidential)
    .fetch_one(&mut *transaction)
    .await?;

    audit::record(
        &mut *transaction,
        AuditEvent {
            actor_type: "user",
            actor_id: Some(actor_id),
            event_type: "admin.client.created",
            target_type: Some("oauth_client"),
            target_id: Some(row.id),
            metadata: json!({
                "client_id": row.client_id,
                "client_name": row.client_name
            }),
        },
    )
    .await?;

    transaction.commit().await?;

    Ok(row.into())
}

pub async fn update_client(
    pool: &PgPool,
    actor_id: Uuid,
    client_id: Uuid,
    input: UpdateClientInput,
) -> Result<Option<AdminOAuthClient>> {
    let mut transaction = pool.begin().await?;
    let client_secret_hash = input
        .client_secret
        .as_deref()
        .map(password::hash_password)
        .transpose()?;

    let row = sqlx::query_as::<_, AdminOAuthClientRow>(
        r#"
        update oauth_clients
        set
            client_secret_hash = coalesce($2, client_secret_hash),
            client_name = coalesce($3, client_name),
            redirect_uris = coalesce($4, redirect_uris),
            scopes = coalesce($5, scopes),
            grant_types = coalesce($6, grant_types),
            auth_method = coalesce($7, auth_method),
            is_confidential = coalesce($8, is_confidential),
            is_active = coalesce($9, is_active),
            updated_at = now()
        where id = $1
        returning
            id,
            client_id,
            client_name,
            redirect_uris,
            scopes,
            grant_types,
            auth_method,
            is_confidential,
            is_active
        "#,
    )
    .bind(client_id)
    .bind(client_secret_hash)
    .bind(input.client_name.as_deref().map(str::trim))
    .bind(input.redirect_uris.map(Json))
    .bind(input.scopes)
    .bind(input.grant_types)
    .bind(input.auth_method.as_deref().map(str::trim))
    .bind(input.is_confidential)
    .bind(input.is_active)
    .fetch_optional(&mut *transaction)
    .await?;

    if let Some(row) = row {
        audit::record(
            &mut *transaction,
            AuditEvent {
                actor_type: "user",
                actor_id: Some(actor_id),
                event_type: "admin.client.updated",
                target_type: Some("oauth_client"),
                target_id: Some(row.id),
                metadata: json!({
                    "client_id": row.client_id,
                    "client_name": row.client_name,
                    "is_active": row.is_active
                }),
            },
        )
        .await?;

        transaction.commit().await?;
        Ok(Some(row.into()))
    } else {
        transaction.rollback().await?;
        Ok(None)
    }
}

pub async fn deactivate_client(pool: &PgPool, actor_id: Uuid, client_id: Uuid) -> Result<bool> {
    let mut transaction = pool.begin().await?;
    let updated_rows = sqlx::query(
        r#"
        update oauth_clients
        set is_active = false, updated_at = now()
        where id = $1
        "#,
    )
    .bind(client_id)
    .execute(&mut *transaction)
    .await?
    .rows_affected();

    if updated_rows == 0 {
        transaction.rollback().await?;
        return Ok(false);
    }

    audit::record(
        &mut *transaction,
        AuditEvent {
            actor_type: "user",
            actor_id: Some(actor_id),
            event_type: "admin.client.deleted",
            target_type: Some("oauth_client"),
            target_id: Some(client_id),
            metadata: json!({}),
        },
    )
    .await?;

    transaction.commit().await?;

    Ok(true)
}

pub async fn list_roles(pool: &PgPool) -> Result<Vec<AdminRole>> {
    let roles = sqlx::query_as(
        r#"
        select id, role_name, description, is_system
        from roles
        order by created_at asc
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(roles)
}

pub async fn create_role(
    pool: &PgPool,
    actor_id: Uuid,
    input: CreateRoleInput,
) -> Result<AdminRole> {
    let mut transaction = pool.begin().await?;
    let role = sqlx::query_as::<_, AdminRole>(
        r#"
        insert into roles (role_name, description, is_system)
        values ($1, $2, $3)
        returning id, role_name, description, is_system
        "#,
    )
    .bind(input.role_name.trim())
    .bind(input.description.as_deref().map(str::trim))
    .bind(input.is_system)
    .fetch_one(&mut *transaction)
    .await?;

    audit::record(
        &mut *transaction,
        AuditEvent {
            actor_type: "user",
            actor_id: Some(actor_id),
            event_type: "admin.role.created",
            target_type: Some("role"),
            target_id: Some(role.id),
            metadata: json!({ "role_name": role.role_name }),
        },
    )
    .await?;

    transaction.commit().await?;

    Ok(role)
}

pub async fn update_role(
    pool: &PgPool,
    actor_id: Uuid,
    role_id: Uuid,
    input: UpdateRoleInput,
) -> Result<Option<AdminRole>> {
    let mut transaction = pool.begin().await?;
    let role = sqlx::query_as::<_, AdminRole>(
        r#"
        update roles
        set
            role_name = coalesce($2, role_name),
            description = coalesce($3, description),
            is_system = coalesce($4, is_system)
        where id = $1
        returning id, role_name, description, is_system
        "#,
    )
    .bind(role_id)
    .bind(input.role_name.as_deref().map(str::trim))
    .bind(input.description.as_deref().map(str::trim))
    .bind(input.is_system)
    .fetch_optional(&mut *transaction)
    .await?;

    if let Some(role) = role {
        audit::record(
            &mut *transaction,
            AuditEvent {
                actor_type: "user",
                actor_id: Some(actor_id),
                event_type: "admin.role.updated",
                target_type: Some("role"),
                target_id: Some(role.id),
                metadata: json!({ "role_name": role.role_name }),
            },
        )
        .await?;

        transaction.commit().await?;
        Ok(Some(role))
    } else {
        transaction.rollback().await?;
        Ok(None)
    }
}

pub async fn delete_role(pool: &PgPool, actor_id: Uuid, role_id: Uuid) -> Result<bool> {
    delete_named_resource(
        pool,
        actor_id,
        role_id,
        "role",
        "roles",
        "admin.role.deleted",
    )
    .await
}

pub async fn list_permissions(pool: &PgPool) -> Result<Vec<AdminPermission>> {
    let permissions = sqlx::query_as(
        r#"
        select id, permission_name, description
        from permissions
        order by created_at asc
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(permissions)
}

pub async fn create_permission(
    pool: &PgPool,
    actor_id: Uuid,
    input: CreatePermissionInput,
) -> Result<AdminPermission> {
    let mut transaction = pool.begin().await?;
    let permission = sqlx::query_as::<_, AdminPermission>(
        r#"
        insert into permissions (permission_name, description)
        values ($1, $2)
        returning id, permission_name, description
        "#,
    )
    .bind(input.permission_name.trim())
    .bind(input.description.as_deref().map(str::trim))
    .fetch_one(&mut *transaction)
    .await?;

    audit::record(
        &mut *transaction,
        AuditEvent {
            actor_type: "user",
            actor_id: Some(actor_id),
            event_type: "admin.permission.created",
            target_type: Some("permission"),
            target_id: Some(permission.id),
            metadata: json!({ "permission_name": permission.permission_name }),
        },
    )
    .await?;

    transaction.commit().await?;

    Ok(permission)
}

pub async fn update_permission(
    pool: &PgPool,
    actor_id: Uuid,
    permission_id: Uuid,
    input: UpdatePermissionInput,
) -> Result<Option<AdminPermission>> {
    let mut transaction = pool.begin().await?;
    let permission = sqlx::query_as::<_, AdminPermission>(
        r#"
        update permissions
        set
            permission_name = coalesce($2, permission_name),
            description = coalesce($3, description)
        where id = $1
        returning id, permission_name, description
        "#,
    )
    .bind(permission_id)
    .bind(input.permission_name.as_deref().map(str::trim))
    .bind(input.description.as_deref().map(str::trim))
    .fetch_optional(&mut *transaction)
    .await?;

    if let Some(permission) = permission {
        audit::record(
            &mut *transaction,
            AuditEvent {
                actor_type: "user",
                actor_id: Some(actor_id),
                event_type: "admin.permission.updated",
                target_type: Some("permission"),
                target_id: Some(permission.id),
                metadata: json!({ "permission_name": permission.permission_name }),
            },
        )
        .await?;

        transaction.commit().await?;
        Ok(Some(permission))
    } else {
        transaction.rollback().await?;
        Ok(None)
    }
}

pub async fn delete_permission(pool: &PgPool, actor_id: Uuid, permission_id: Uuid) -> Result<bool> {
    delete_named_resource(
        pool,
        actor_id,
        permission_id,
        "permission",
        "permissions",
        "admin.permission.deleted",
    )
    .await
}

pub async fn list_policies(pool: &PgPool) -> Result<Vec<AdminPolicyRule>> {
    let rows = sqlx::query_as::<_, AdminPolicyRuleRow>(
        r#"
        select
            id,
            rule_name,
            effect,
            resource_pattern,
            action_pattern,
            condition_expression,
            priority,
            is_active,
            description
        from policy_rules
        order by priority desc, created_at asc
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(AdminPolicyRule::from).collect())
}

pub async fn create_policy(
    pool: &PgPool,
    actor_id: Uuid,
    input: CreatePolicyInput,
) -> Result<AdminPolicyRule> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query_as::<_, AdminPolicyRuleRow>(
        r#"
        insert into policy_rules (
            rule_name,
            effect,
            resource_pattern,
            action_pattern,
            condition_expression,
            priority,
            description
        )
        values ($1, $2, $3, $4, $5, $6, $7)
        returning
            id,
            rule_name,
            effect,
            resource_pattern,
            action_pattern,
            condition_expression,
            priority,
            is_active,
            description
        "#,
    )
    .bind(input.rule_name.trim())
    .bind(input.effect.trim())
    .bind(input.resource_pattern.trim())
    .bind(input.action_pattern.trim())
    .bind(Json(input.condition_expression))
    .bind(input.priority)
    .bind(input.description.as_deref().map(str::trim))
    .fetch_one(&mut *transaction)
    .await?;

    audit::record(
        &mut *transaction,
        AuditEvent {
            actor_type: "user",
            actor_id: Some(actor_id),
            event_type: "admin.policy.created",
            target_type: Some("policy_rule"),
            target_id: Some(row.id),
            metadata: json!({ "rule_name": row.rule_name }),
        },
    )
    .await?;

    transaction.commit().await?;

    Ok(row.into())
}

pub async fn update_policy(
    pool: &PgPool,
    actor_id: Uuid,
    policy_id: Uuid,
    input: UpdatePolicyInput,
) -> Result<Option<AdminPolicyRule>> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query_as::<_, AdminPolicyRuleRow>(
        r#"
        update policy_rules
        set
            rule_name = coalesce($2, rule_name),
            effect = coalesce($3, effect),
            resource_pattern = coalesce($4, resource_pattern),
            action_pattern = coalesce($5, action_pattern),
            condition_expression = coalesce($6, condition_expression),
            priority = coalesce($7, priority),
            is_active = coalesce($8, is_active),
            description = coalesce($9, description),
            updated_at = now()
        where id = $1
        returning
            id,
            rule_name,
            effect,
            resource_pattern,
            action_pattern,
            condition_expression,
            priority,
            is_active,
            description
        "#,
    )
    .bind(policy_id)
    .bind(input.rule_name.as_deref().map(str::trim))
    .bind(input.effect.as_deref().map(str::trim))
    .bind(input.resource_pattern.as_deref().map(str::trim))
    .bind(input.action_pattern.as_deref().map(str::trim))
    .bind(input.condition_expression.map(Json))
    .bind(input.priority)
    .bind(input.is_active)
    .bind(input.description.as_deref().map(str::trim))
    .fetch_optional(&mut *transaction)
    .await?;

    if let Some(row) = row {
        audit::record(
            &mut *transaction,
            AuditEvent {
                actor_type: "user",
                actor_id: Some(actor_id),
                event_type: "admin.policy.updated",
                target_type: Some("policy_rule"),
                target_id: Some(row.id),
                metadata: json!({ "rule_name": row.rule_name, "is_active": row.is_active }),
            },
        )
        .await?;

        transaction.commit().await?;
        Ok(Some(row.into()))
    } else {
        transaction.rollback().await?;
        Ok(None)
    }
}

pub async fn delete_policy(pool: &PgPool, actor_id: Uuid, policy_id: Uuid) -> Result<bool> {
    delete_named_resource(
        pool,
        actor_id,
        policy_id,
        "policy_rule",
        "policy_rules",
        "admin.policy.deleted",
    )
    .await
}

async fn delete_named_resource(
    pool: &PgPool,
    actor_id: Uuid,
    target_id: Uuid,
    target_type: &'static str,
    table_name: &'static str,
    event_type: &'static str,
) -> Result<bool> {
    let mut transaction = pool.begin().await?;
    let query = format!("delete from {table_name} where id = $1");
    let deleted_rows = sqlx::query(&query)
        .bind(target_id)
        .execute(&mut *transaction)
        .await?
        .rows_affected();

    if deleted_rows == 0 {
        transaction.rollback().await?;
        return Ok(false);
    }

    audit::record(
        &mut *transaction,
        AuditEvent {
            actor_type: "user",
            actor_id: Some(actor_id),
            event_type,
            target_type: Some(target_type),
            target_id: Some(target_id),
            metadata: json!({}),
        },
    )
    .await?;

    transaction.commit().await?;
    Ok(true)
}

impl From<AdminOAuthClientRow> for AdminOAuthClient {
    fn from(value: AdminOAuthClientRow) -> Self {
        Self {
            id: value.id,
            client_id: value.client_id,
            client_name: value.client_name,
            redirect_uris: value.redirect_uris.0,
            scopes: value.scopes,
            grant_types: value.grant_types,
            auth_method: value.auth_method,
            is_confidential: value.is_confidential,
            is_active: value.is_active,
        }
    }
}

impl From<AdminPolicyRuleRow> for AdminPolicyRule {
    fn from(value: AdminPolicyRuleRow) -> Self {
        Self {
            id: value.id,
            rule_name: value.rule_name,
            effect: value.effect,
            resource_pattern: value.resource_pattern,
            action_pattern: value.action_pattern,
            condition_expression: value.condition_expression.0,
            priority: value.priority,
            is_active: value.is_active,
            description: value.description,
        }
    }
}
