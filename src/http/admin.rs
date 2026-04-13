use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use axum_extra::extract::CookieJar;
use sqlx::types::Uuid;

use crate::{
    admin::{
        self, CreateClientInput, CreatePermissionInput, CreatePolicyInput, CreateRoleInput,
        CreateUserInput, UpdateClientInput, UpdatePermissionInput, UpdatePolicyInput,
        UpdateRoleInput, UpdateUserInput,
    },
    http::auth::SESSION_COOKIE_NAME,
    identity::session,
    oauth,
    state::AppState,
};

pub fn build_router() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list_users).post(create_user))
        .route(
            "/admin/users/{user_id}",
            patch(update_user).delete(delete_user),
        )
        .route("/admin/clients", get(list_clients).post(create_client))
        .route(
            "/admin/clients/{client_id}",
            patch(update_client).delete(delete_client),
        )
        .route("/admin/roles", get(list_roles).post(create_role))
        .route(
            "/admin/roles/{role_id}",
            patch(update_role).delete(delete_role),
        )
        .route(
            "/admin/permissions",
            get(list_permissions).post(create_permission),
        )
        .route(
            "/admin/permissions/{permission_id}",
            patch(update_permission).delete(delete_permission),
        )
        .route("/admin/policies", get(list_policies).post(create_policy))
        .route(
            "/admin/policies/{policy_id}",
            patch(update_policy).delete(delete_policy),
        )
}

async fn list_users(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<admin::AdminUser>>, StatusCode> {
    require_admin_user_id(&state, &jar).await?;

    let users = admin::list_users(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(users))
}

async fn create_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<CreateUserInput>,
) -> Result<(StatusCode, Json<admin::AdminUser>), StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    if payload.username.trim().is_empty()
        || payload.email.trim().is_empty()
        || payload.password.is_empty()
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    let user = admin::create_user(&state.db_pool, actor_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(user)))
}

async fn update_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<UpdateUserInput>,
) -> Result<Json<admin::AdminUser>, StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    let Some(user) = admin::update_user(&state.db_pool, actor_id, user_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Err(StatusCode::NOT_FOUND);
    };

    Ok(Json(user))
}

async fn delete_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    let deleted = admin::deactivate_user(&state.db_pool, actor_id, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn list_clients(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<admin::AdminOAuthClient>>, StatusCode> {
    require_admin_user_id(&state, &jar).await?;

    let clients = admin::list_clients(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(clients))
}

async fn create_client(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<CreateClientInput>,
) -> Result<(StatusCode, Json<admin::AdminOAuthClient>), StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    if payload.client_id.trim().is_empty() || payload.client_name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let client = admin::create_client(&state.db_pool, actor_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(client)))
}

async fn update_client(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(client_id): Path<Uuid>,
    Json(payload): Json<UpdateClientInput>,
) -> Result<Json<admin::AdminOAuthClient>, StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    let Some(client) = admin::update_client(&state.db_pool, actor_id, client_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Err(StatusCode::NOT_FOUND);
    };

    Ok(Json(client))
}

async fn delete_client(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(client_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;
    let deleted = admin::deactivate_client(&state.db_pool, actor_id, client_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn list_roles(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<admin::AdminRole>>, StatusCode> {
    require_admin_user_id(&state, &jar).await?;

    let roles = admin::list_roles(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(roles))
}

async fn create_role(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<CreateRoleInput>,
) -> Result<(StatusCode, Json<admin::AdminRole>), StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    if payload.role_name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let role = admin::create_role(&state.db_pool, actor_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(role)))
}

async fn update_role(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(role_id): Path<Uuid>,
    Json(payload): Json<UpdateRoleInput>,
) -> Result<Json<admin::AdminRole>, StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    let Some(role) = admin::update_role(&state.db_pool, actor_id, role_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Err(StatusCode::NOT_FOUND);
    };

    Ok(Json(role))
}

async fn delete_role(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(role_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;
    let deleted = admin::delete_role(&state.db_pool, actor_id, role_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn list_permissions(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<admin::AdminPermission>>, StatusCode> {
    require_admin_user_id(&state, &jar).await?;

    let permissions = admin::list_permissions(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(permissions))
}

async fn create_permission(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<CreatePermissionInput>,
) -> Result<(StatusCode, Json<admin::AdminPermission>), StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    if payload.permission_name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let permission = admin::create_permission(&state.db_pool, actor_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(permission)))
}

async fn update_permission(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(permission_id): Path<Uuid>,
    Json(payload): Json<UpdatePermissionInput>,
) -> Result<Json<admin::AdminPermission>, StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    let Some(permission) =
        admin::update_permission(&state.db_pool, actor_id, permission_id, payload)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Err(StatusCode::NOT_FOUND);
    };

    Ok(Json(permission))
}

async fn delete_permission(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(permission_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;
    let deleted = admin::delete_permission(&state.db_pool, actor_id, permission_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn list_policies(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<admin::AdminPolicyRule>>, StatusCode> {
    require_admin_user_id(&state, &jar).await?;

    let policies = admin::list_policies(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(policies))
}

async fn create_policy(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<CreatePolicyInput>,
) -> Result<(StatusCode, Json<admin::AdminPolicyRule>), StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    if payload.rule_name.trim().is_empty()
        || payload.effect.trim().is_empty()
        || payload.resource_pattern.trim().is_empty()
        || payload.action_pattern.trim().is_empty()
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    let policy = admin::create_policy(&state.db_pool, actor_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(policy)))
}

async fn update_policy(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(policy_id): Path<Uuid>,
    Json(payload): Json<UpdatePolicyInput>,
) -> Result<Json<admin::AdminPolicyRule>, StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;

    let Some(policy) = admin::update_policy(&state.db_pool, actor_id, policy_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Err(StatusCode::NOT_FOUND);
    };

    Ok(Json(policy))
}

async fn delete_policy(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(policy_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let actor_id = require_admin_user_id(&state, &jar).await?;
    let deleted = admin::delete_policy(&state.db_pool, actor_id, policy_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn require_admin_user_id(state: &AppState, jar: &CookieJar) -> Result<Uuid, StatusCode> {
    let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let session_token_hash = session::hash_session_token(session_cookie.value());
    let Some(user_id) = oauth::find_active_session_user_id(&state.db_pool, &session_token_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let has_permission =
        admin::user_has_permission(&state.db_pool, user_id, admin::ADMIN_PERMISSION)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !has_permission {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(user_id)
}
