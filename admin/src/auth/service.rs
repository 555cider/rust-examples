use crate::{
    auth::models::AuthnUser,
    config::env_loader::AppConfig,
    errors::AppError,
    models::{dto::auth::CurrentUserResponse, dto::auth::LoginRequest, dto::auth::RegisterRequest},
    repositories::{
        auth::AuthRepository, permission::PermissionRepository, user::UserRepository,
        role::RoleRepository,
    },
    services::history::HistoryService,
    utils::{password, tokens},
};
use std::sync::Arc;
use tracing::{error, info, warn};
use validator::Validate;

pub struct AuthService {
    auth_repo: AuthRepository,
    user_repo: UserRepository,
    role_repo: RoleRepository,
    permission_repo: PermissionRepository,
    history: Arc<HistoryService>,
}

impl AuthService {
    pub fn new(
        auth_repo: AuthRepository,
        user_repo: UserRepository,
        role_repo: RoleRepository,
        permission_repo: PermissionRepository,
        history: Arc<HistoryService>,
    ) -> Self {
        Self {
            auth_repo,
            user_repo,
            role_repo,
            permission_repo,
            history,
        }
    }

    pub async fn login(
        &self,
        config: &AppConfig,
        req: LoginRequest,
        ip_address: Option<String>,
        _user_agent: Option<String>,
    ) -> Result<(String, String), AppError> {
        req.validate()?;
        info!("Login attempt for username: {}", req.username);

        let user = match self.auth_repo.find_user_by_username(&req.username).await? {
            Some(user) => user,
            None => {
                // Log failed login attempt (user not found)
                if let Err(e) = self
                    .history
                    .log_login_failed(
                        &req.username,
                        "user_not_found".to_string(),
                        ip_address.clone(),
                    )
                    .await
                {
                    error!("Failed to log failed login attempt: {}", e);
                }

                return Err(AppError::Unauthorized(
                    "Invalid username or password".to_string(),
                ));
            }
        };

        if !password::verify_password(&req.password, &user.password).await? {
            // Log failed login attempt (wrong password)
            if let Err(e) = self
                .history
                .log_login_failed(
                    &req.username,
                    "invalid_password".to_string(),
                    ip_address.clone(),
                )
                .await
            {
                error!("Failed to log failed login attempt: {}", e);
            }

            return Err(AppError::Unauthorized(
                "Invalid username or password".to_string(),
            ));
        }

        // Clone username before moving it
        let username = user.username.clone();

        // Log successful login
        if let Err(e) = self
            .history
            .log_login_success(user.id, Some(username.clone()), ip_address.clone())
            .await
        {
            error!("Failed to log successful login: {}", e);
        }

        info!("User {} logged in successfully", user.id);
        let access_token = tokens::generate_access_token(config, user.id, &user.role, &username)?;

        let refresh_token = tokens::generate_refresh_token(config, user.id, &user.role, &username)?;

        // Save refresh token to database
        self.auth_repo
            .save_refresh_token(user.id, &refresh_token)
            .await?;

        // Update last login time
        self.user_repo.update_last_login(user.id).await?;

        Ok((access_token, refresh_token))
    }

    pub async fn get_authn_user(&self, user_id: i64) -> Result<AuthnUser, AppError> {
        let user = self.user_repo.find_by_id(user_id).await?;
        let _role = self.role_repo.find_by_id(user.role_id).await?;
        let permissions = self
            .permission_repo
            .find_by_role_id(user.role_id)
            .await?;
        let permissions: Vec<String> = permissions.into_iter().map(|p| p.code).collect();

        Ok(AuthnUser {
            id: user.id,
            role_id: user.role_id,
            username: user.username,
            permissions: Arc::new(std::collections::HashSet::from_iter(permissions)),
        })
    }

    pub async fn register(
        &self,
        req: RegisterRequest,
        ip_address: Option<String>,
        _user_agent: Option<String>,
    ) -> Result<i64, AppError> {
        req.validate()?;
        info!("Register request for username: {}", req.username);

        // Check if username already exists
        if self
            .auth_repo
            .find_user_by_username(&req.username)
            .await?
            .is_some()
        {
            return Err(AppError::BadRequest("Username already exists".to_string()));
        }

        // Check if role exists and get its name
        let _role_info = self
            .role_repo
            .find_by_id(req.role_id)
            .await
            .map_err(|_| AppError::BadRequest("Invalid role".to_string()))?;

        let hashed_password = password::hash_password(&req.password).await?;

        // Clone username to avoid moving it
        let username = req.username.clone();

        let user_id = self
            .auth_repo
            .create_user(
                username,
                req.email,
                hashed_password,
                req.role_id,
                true, // is_active
            )
            .await?;

        // Log the registration
        self.history
            .create_log(
                Some(user_id),
                "user_created",
                Some(user_id),
                Some(serde_json::json!({ "username": &req.username })),
                ip_address,
                None, // user_agent
            )
            .await?;

        info!("User registered successfully: {}", user_id);
        Ok(user_id)
    }

    pub async fn refresh_access_token(
        &self,
        config: &AppConfig,
        refresh_token: String,
        ip_address: Option<String>,
        _user_agent: Option<String>,
    ) -> Result<(String, String), AppError> {
        let claims = match tokens::validate_token(&refresh_token) {
            Ok(claims) => claims,
            Err(e) => {
                warn!("Invalid refresh token: {}", e);
                return Err(AppError::Unauthorized("Invalid refresh token".to_string()));
            }
        };

        // Verify the token is not expired
        if claims.exp < chrono::Utc::now().timestamp() as usize {
            warn!("Expired refresh token used for user: {}", claims.sub);
            return Err(AppError::Unauthorized("Refresh token expired".to_string()));
        }

        // Get the user from the database
        let user = self.user_repo.find_by_id(claims.sub).await.map_err(|e| {
            warn!("User not found for refresh token: {} - {}", claims.sub, e);
            AppError::Unauthorized("User not found".to_string())
        })?;

        // Get role info
        let role_info = self
            .role_repo
            .find_by_id(user.role_id)
            .await?;

        // Get role name or default to "user" if not found
        let role_name = role_info.name.as_str();

        // Clone username to avoid moving it
        let username = user.username.clone();

        // Generate new tokens
        let access_token =
            tokens::generate_access_token(config, user.id, role_name, &username)?;

        let refresh_token =
            tokens::generate_refresh_token(config, user.id, role_name, &username)?;

        // Log token refresh
        if let Err(e) = self
            .history
            .create_log(
                Some(user.id),
                "token_refresh",
                Some(user.id),
                None,
                ip_address,
                None, // user_agent
            )
            .await
        {
            error!("Failed to log token refresh: {}", e);
        }

        info!("Refreshed tokens for user: {}", user.id);
        Ok((access_token, refresh_token))
    }

    pub async fn get_current_user(
        &self,
        current_user: AuthnUser,
    ) -> Result<CurrentUserResponse, AppError> {
        let role_info = self
            .role_repo
            .find_by_id(current_user.role_id)
            .await?;

        Ok(CurrentUserResponse {
            id: current_user.id,
            username: current_user.username,
            role_id: current_user.role_id,
            role: Some(role_info),
            permissions: current_user.permissions.iter().cloned().collect(),
        })
    }
}
