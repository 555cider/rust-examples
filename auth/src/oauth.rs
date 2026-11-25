use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{
    FromRow, PgPool,
    postgres::PgQueryResult,
    types::{Json, Uuid},
};

use crate::{
    audit::{self, AuditEvent},
    error::{AppError, Result},
    identity::{password, session},
};

const AUTHORIZATION_CODE_TTL_SECONDS: i32 = 300;
const REFRESH_TOKEN_TTL_SECONDS: i32 = 60 * 60 * 24 * 30;

#[derive(Debug, Clone, FromRow)]
pub struct OAuthClient {
    pub id: Uuid,
    pub client_id: String,
    pub client_secret_hash: Option<String>,
    pub redirect_uris: Json<Vec<String>>,
    pub scopes: Vec<String>,
    pub grant_types: Vec<String>,
    pub auth_method: String,
    pub is_confidential: bool,
    pub is_active: bool,
}

#[derive(Debug, FromRow)]
pub struct AuthorizationCodeRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub client_id: Uuid,
    pub scope: String,
    pub nonce: Option<String>,
    pub pkce_code_challenge: Option<String>,
    pub pkce_code_challenge_method: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct RefreshTokenRecord {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub client_id: Uuid,
    pub scope: String,
    pub rotated_at_present: bool,
    pub reused_at_present: bool,
}

#[derive(Debug)]
pub enum OAuthError {
    InvalidRequest(&'static str),
    InvalidClient,
    InvalidGrant,
    UnauthorizedClient,
    UnsupportedGrantType,
}

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

pub enum ClientAuthentication {
    None,
    Basic {
        client_id: String,
        client_secret: String,
    },
    Post {
        client_id: String,
        client_secret: String,
    },
}

pub struct AuthorizationRequest<'a> {
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub scope: &'a str,
    pub code_challenge: Option<&'a str>,
    pub code_challenge_method: Option<&'a str>,
}

pub struct StoredAuthorizationCode {
    pub plain_text: String,
    pub hash: String,
}

pub struct StoredRefreshToken {
    pub id: Uuid,
    pub plain_text: String,
    pub hash: String,
}

pub async fn find_oauth_client(pool: &PgPool, client_id: &str) -> Result<Option<OAuthClient>> {
    let client = sqlx::query_as(
        r#"
        select
            id,
            client_id,
            client_secret_hash,
            redirect_uris,
            scopes,
            grant_types,
            auth_method,
            is_confidential,
            is_active
        from oauth_clients
        where client_id = $1
          and is_active = true
        "#,
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?;

    Ok(client)
}

pub fn validate_authorization_request(
    client: &OAuthClient,
    request: &AuthorizationRequest<'_>,
) -> std::result::Result<(), OAuthError> {
    if !client
        .grant_types
        .iter()
        .any(|grant| grant == "authorization_code")
    {
        return Err(OAuthError::UnauthorizedClient);
    }
    if !client
        .redirect_uris
        .0
        .iter()
        .any(|registered| registered == request.redirect_uri)
    {
        return Err(OAuthError::InvalidRequest("redirect_uri"));
    }
    if !scope_is_allowed(request.scope, &client.scopes) {
        return Err(OAuthError::InvalidRequest("scope"));
    }
    if !client.is_confidential && request.code_challenge.is_none() {
        return Err(OAuthError::InvalidRequest("code_challenge"));
    }
    if let Some(method) = request.code_challenge_method {
        if method != "S256" && method != "plain" {
            return Err(OAuthError::InvalidRequest("code_challenge_method"));
        }
    }

    Ok(())
}

pub async fn find_active_session_user_id(
    pool: &PgPool,
    session_token_hash: &str,
) -> Result<Option<Uuid>> {
    let user_id = sqlx::query_scalar(
        r#"
        select user_id
        from sessions
        where session_token_hash = $1
          and revoked_at is null
          and expires_at > now()
        "#,
    )
    .bind(session_token_hash)
    .fetch_optional(pool)
    .await?;

    Ok(user_id)
}

pub async fn store_authorization_code(
    pool: &PgPool,
    client_id: Uuid,
    user_id: Uuid,
    redirect_uri: &str,
    scope: &str,
    nonce: Option<&str>,
    code_challenge: Option<&str>,
    code_challenge_method: Option<&str>,
) -> Result<StoredAuthorizationCode> {
    let plain_text = issue_opaque_secret();
    let hash = session::hash_session_token(&plain_text);

    sqlx::query(
        r#"
        insert into oauth_authorization_codes (
            code_hash,
            client_id,
            user_id,
            redirect_uri,
            scope,
            nonce,
            pkce_code_challenge,
            pkce_code_challenge_method,
            expires_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, now() + ($9 * interval '1 second'))
        "#,
    )
    .bind(&hash)
    .bind(client_id)
    .bind(user_id)
    .bind(redirect_uri)
    .bind(scope)
    .bind(nonce)
    .bind(code_challenge)
    .bind(code_challenge_method)
    .bind(AUTHORIZATION_CODE_TTL_SECONDS)
    .execute(pool)
    .await?;

    Ok(StoredAuthorizationCode { plain_text, hash })
}

pub async fn consume_authorization_code(
    pool: &PgPool,
    client: &OAuthClient,
    code: &str,
    redirect_uri: &str,
    code_verifier: Option<&str>,
) -> std::result::Result<AuthorizationCodeRecord, OAuthError> {
    let code_hash = session::hash_session_token(code);

    let mut transaction = pool.begin().await.map_err(internal_sqlx_error)?;
    let record = sqlx::query_as::<_, AuthorizationCodeRecord>(
        r#"
        select
            id,
            user_id,
            client_id,
            scope,
            nonce,
            pkce_code_challenge,
            pkce_code_challenge_method
        from oauth_authorization_codes
        where code_hash = $1
          and client_id = $2
          and redirect_uri = $3
          and consumed_at is null
          and expires_at > now()
        "#,
    )
    .bind(code_hash)
    .bind(client.id)
    .bind(redirect_uri)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_sqlx_error)?
    .ok_or(OAuthError::InvalidGrant)?;

    validate_pkce(
        record.pkce_code_challenge.as_deref(),
        record.pkce_code_challenge_method.as_deref(),
        code_verifier,
    )?;

    let updated: PgQueryResult = sqlx::query(
        r#"
        update oauth_authorization_codes
        set consumed_at = now()
        where id = $1
          and consumed_at is null
        "#,
    )
    .bind(record.id)
    .execute(&mut *transaction)
    .await
    .map_err(internal_sqlx_error)?;

    if updated.rows_affected() != 1 {
        return Err(OAuthError::InvalidGrant);
    }

    transaction.commit().await.map_err(internal_sqlx_error)?;

    Ok(record)
}

pub async fn issue_refresh_token(
    pool: &PgPool,
    client_id: Uuid,
    user_id: Option<Uuid>,
    scope: &str,
    parent_token_id: Option<Uuid>,
) -> Result<StoredRefreshToken> {
    let plain_text = issue_opaque_secret();
    let hash = session::hash_session_token(&plain_text);

    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        insert into oauth_refresh_tokens (
            refresh_token_hash,
            client_id,
            user_id,
            parent_token_id,
            scope,
            expires_at
        )
        values ($1, $2, $3, $4, $5, now() + ($6 * interval '1 second'))
        returning id
        "#,
    )
    .bind(&hash)
    .bind(client_id)
    .bind(user_id)
    .bind(parent_token_id)
    .bind(scope)
    .bind(REFRESH_TOKEN_TTL_SECONDS)
    .fetch_one(pool)
    .await?;

    Ok(StoredRefreshToken {
        id,
        plain_text,
        hash,
    })
}

pub async fn rotate_refresh_token(
    pool: &PgPool,
    client: &OAuthClient,
    refresh_token: &str,
) -> std::result::Result<(RefreshTokenRecord, StoredRefreshToken), OAuthError> {
    if !client
        .grant_types
        .iter()
        .any(|grant| grant == "refresh_token")
    {
        return Err(OAuthError::UnauthorizedClient);
    }

    let refresh_token_hash = session::hash_session_token(refresh_token);
    let mut transaction = pool.begin().await.map_err(internal_sqlx_error)?;
    let record = sqlx::query_as::<_, RefreshTokenRecord>(
        r#"
        select
            id,
            user_id,
            client_id,
            scope,
            rotated_at is not null as rotated_at_present,
            reused_at is not null as reused_at_present
        from oauth_refresh_tokens
        where refresh_token_hash = $1
          and client_id = $2
          and revoked_at is null
          and expires_at > now()
        "#,
    )
    .bind(&refresh_token_hash)
    .bind(client.id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal_sqlx_error)?
    .ok_or(OAuthError::InvalidGrant)?;

    if record.rotated_at_present || record.reused_at_present {
        audit::record(
            &mut *transaction,
            AuditEvent {
                actor_type: if record.user_id.is_some() {
                    "user"
                } else {
                    "client"
                },
                actor_id: record.user_id,
                event_type: "oauth.token.refresh_reuse_detected",
                target_type: Some("oauth_client"),
                target_id: Some(client.id),
                metadata: json!({
                    "client_id": client.client_id,
                    "scope": record.scope
                }),
            },
        )
        .await
        .map_err(internal_error)?;

        sqlx::query(
            r#"
            update oauth_refresh_tokens
            set reused_at = coalesce(reused_at, now())
            where id = $1
            "#,
        )
        .bind(record.id)
        .execute(&mut *transaction)
        .await
        .map_err(internal_sqlx_error)?;
        transaction.commit().await.map_err(internal_sqlx_error)?;
        return Err(OAuthError::InvalidGrant);
    }

    let updated: PgQueryResult = sqlx::query(
        r#"
        update oauth_refresh_tokens
        set rotated_at = now()
        where id = $1
          and rotated_at is null
          and reused_at is null
        "#,
    )
    .bind(record.id)
    .execute(&mut *transaction)
    .await
    .map_err(internal_sqlx_error)?;

    if updated.rows_affected() != 1 {
        return Err(OAuthError::InvalidGrant);
    }

    let replacement_plain = issue_opaque_secret();
    let replacement_hash = session::hash_session_token(&replacement_plain);
    let replacement_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        insert into oauth_refresh_tokens (
            refresh_token_hash,
            client_id,
            user_id,
            parent_token_id,
            scope,
            expires_at
        )
        values ($1, $2, $3, $4, $5, now() + ($6 * interval '1 second'))
        returning id
        "#,
    )
    .bind(&replacement_hash)
    .bind(record.client_id)
    .bind(record.user_id)
    .bind(record.id)
    .bind(&record.scope)
    .bind(REFRESH_TOKEN_TTL_SECONDS)
    .fetch_one(&mut *transaction)
    .await
    .map_err(internal_sqlx_error)?;

    audit::record(
        &mut *transaction,
        AuditEvent {
            actor_type: if record.user_id.is_some() {
                "user"
            } else {
                "client"
            },
            actor_id: record.user_id,
            event_type: "oauth.token.refresh_rotated",
            target_type: Some("oauth_client"),
            target_id: Some(client.id),
            metadata: json!({
                "client_id": client.client_id,
                "scope": record.scope
            }),
        },
    )
    .await
    .map_err(internal_error)?;

    transaction.commit().await.map_err(internal_sqlx_error)?;

    Ok((
        record,
        StoredRefreshToken {
            id: replacement_id,
            plain_text: replacement_plain,
            hash: replacement_hash,
        },
    ))
}

pub async fn authenticate_client(
    pool: &PgPool,
    auth: ClientAuthentication,
    request_client_id: Option<&str>,
) -> std::result::Result<OAuthClient, OAuthError> {
    let (client_id, client_secret) = match auth {
        ClientAuthentication::None => (
            request_client_id
                .ok_or(OAuthError::InvalidClient)?
                .to_owned(),
            None,
        ),
        ClientAuthentication::Basic {
            client_id,
            client_secret,
        }
        | ClientAuthentication::Post {
            client_id,
            client_secret,
        } => (client_id, Some(client_secret)),
    };

    let client = find_oauth_client(pool, &client_id)
        .await
        .map_err(internal_error)?
        .ok_or(OAuthError::InvalidClient)?;
    if !client.is_active {
        return Err(OAuthError::InvalidClient);
    }

    match client.auth_method.as_str() {
        "none" => {
            if client_secret.is_some() {
                return Err(OAuthError::InvalidClient);
            }
        }
        "client_secret_basic" | "client_secret_post" => {
            let provided_secret = client_secret.ok_or(OAuthError::InvalidClient)?;
            let stored_secret_hash = client
                .client_secret_hash
                .as_deref()
                .ok_or(OAuthError::InvalidClient)?;
            let secret_matches = password::verify_password(&provided_secret, stored_secret_hash)
                .map_err(internal_error)?;
            if !secret_matches {
                return Err(OAuthError::InvalidClient);
            }
        }
        _ => return Err(OAuthError::InvalidClient),
    }

    Ok(client)
}

pub fn parse_client_authentication(
    authorization_header: Option<&str>,
    form_client_id: Option<&str>,
    form_client_secret: Option<&str>,
) -> std::result::Result<ClientAuthentication, OAuthError> {
    if let Some(header) = authorization_header {
        let encoded = header
            .strip_prefix("Basic ")
            .ok_or(OAuthError::InvalidClient)?;
        let decoded_bytes = STANDARD
            .decode(encoded)
            .map_err(|_| OAuthError::InvalidClient)?;
        let decoded = String::from_utf8(decoded_bytes).map_err(|_| OAuthError::InvalidClient)?;
        let (client_id, client_secret) =
            decoded.split_once(':').ok_or(OAuthError::InvalidClient)?;

        return Ok(ClientAuthentication::Basic {
            client_id: client_id.to_owned(),
            client_secret: client_secret.to_owned(),
        });
    }

    match (form_client_id, form_client_secret) {
        (Some(client_id), Some(client_secret)) => Ok(ClientAuthentication::Post {
            client_id: client_id.to_owned(),
            client_secret: client_secret.to_owned(),
        }),
        (Some(_), None) => Ok(ClientAuthentication::None),
        (None, None) => Ok(ClientAuthentication::None),
        (None, Some(_)) => Err(OAuthError::InvalidClient),
    }
}

fn validate_pkce(
    stored_challenge: Option<&str>,
    stored_method: Option<&str>,
    code_verifier: Option<&str>,
) -> std::result::Result<(), OAuthError> {
    match (stored_challenge, stored_method) {
        (None, None) => Ok(()),
        (Some(challenge), Some("plain")) => {
            let verifier = code_verifier.ok_or(OAuthError::InvalidGrant)?;
            if verifier == challenge {
                Ok(())
            } else {
                Err(OAuthError::InvalidGrant)
            }
        }
        (Some(challenge), Some("S256")) => {
            let verifier = code_verifier.ok_or(OAuthError::InvalidGrant)?;
            let computed = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(Sha256::digest(verifier.as_bytes()));
            if computed == challenge {
                Ok(())
            } else {
                Err(OAuthError::InvalidGrant)
            }
        }
        _ => Err(OAuthError::InvalidGrant),
    }
}

fn scope_is_allowed(requested_scope: &str, allowed_scopes: &[String]) -> bool {
    requested_scope
        .split_whitespace()
        .all(|scope| allowed_scopes.iter().any(|allowed| allowed == scope))
}

fn issue_opaque_secret() -> String {
    session::issue_session_token().plain_text
}

fn internal_error(error: AppError) -> OAuthError {
    tracing::error!(?error, "oauth internal error");
    OAuthError::InvalidRequest("server_error")
}

fn internal_sqlx_error(error: sqlx::Error) -> OAuthError {
    internal_error(error.into())
}
