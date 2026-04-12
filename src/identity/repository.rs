use sqlx::{FromRow, PgPool, types::Uuid};

use crate::error::Result;

#[derive(Debug, FromRow)]
pub struct UserPasswordCredential {
    pub user_id: Uuid,
    pub password_hash: String,
}

pub async fn find_user_password_credential(
    pool: &PgPool,
    username: &str,
) -> Result<Option<UserPasswordCredential>> {
    let credential = sqlx::query_as(
        r#"
        select users.id as user_id, password_credentials.password_hash
        from users
        inner join password_credentials on password_credentials.user_id = users.id
        where users.username = $1
          and users.is_active = true
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    Ok(credential)
}

pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    session_token_hash: &str,
    ttl_seconds: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        insert into sessions (user_id, session_token_hash, expires_at)
        values ($1, $2, now() + ($3 * interval '1 second'))
        "#,
    )
    .bind(user_id)
    .bind(session_token_hash)
    .bind(ttl_seconds)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn revoke_session(pool: &PgPool, session_token_hash: &str) -> Result<()> {
    sqlx::query(
        r#"
        update sessions
        set revoked_at = now()
        where session_token_hash = $1
          and revoked_at is null
        "#,
    )
    .bind(session_token_hash)
    .execute(pool)
    .await?;

    Ok(())
}
