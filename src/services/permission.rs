use crate::{
    config::security::SecurityConfig,
    errors::AppError,
    models::{
        dto::common::ListQueryParams, dto::permission::CreatePermissionRequest,
        dto::permission::PermissionResponse, dto::permission::UpdatePermissionRequest,
    },
    repositories::{permission::PermissionRepository, user::UserRepository},
};
use std::sync::Arc;
use validator::Validate;

pub struct PermissionService {
    permission_repo: PermissionRepository,
    user_repo: UserRepository,
    security_config: Arc<SecurityConfig>,
}

impl PermissionService {
    pub fn new(permission_repo: PermissionRepository, user_repo: UserRepository) -> Self {
        Self {
            permission_repo,
            user_repo,
            security_config: Arc::new(SecurityConfig::from_env()),
        }
    }

    pub fn with_security_config(
        permission_repo: PermissionRepository,
        user_repo: UserRepository,
        security_config: Arc<SecurityConfig>,
    ) -> Self {
        Self {
            permission_repo,
            user_repo,
            security_config,
        }
    }

    pub async fn create_permission(&self, req: CreatePermissionRequest) -> Result<i64, AppError> {
        req.validate()?;

        // Validate permission code against allowed patterns
        if let Err(e) = self.security_config.validate_permission_code(&req.code) {
            return Err(AppError::BadRequest(e));
        }

        self.permission_repo.create(req).await
    }

    pub async fn get_permissions(
        &self,
        query: ListQueryParams,
    ) -> Result<Vec<PermissionResponse>, AppError> {
        self.permission_repo.find_all(query).await
    }

    pub async fn get_permission_by_id(&self, id: i64) -> Result<PermissionResponse, AppError> {
        self.permission_repo.find_by_id(id).await
    }

    /// Check if a user has a specific permission
    pub async fn has_permission(
        &self,
        user_id: i64,
        permission_name: &str,
    ) -> Result<bool, AppError> {
        // Check if the permission exists
        let permission = self.permission_repo.find_by_name(permission_name).await?;
        if permission.is_none() {
            return Ok(false);
        }

        // Check if user is super admin using security config
        let user = self.user_repo.find_by_id(user_id).await?;
        Ok(self
            .security_config
            .is_super_admin_role_id(user.role_id))
    }

    pub async fn count_permissions(&self) -> Result<i64, AppError> {
        self.permission_repo.count().await
    }

    pub async fn update_permission(
        &self,
        id: i64,
        req: UpdatePermissionRequest,
    ) -> Result<PermissionResponse, AppError> {
        if let Some(name) = &req.name {
            if name.trim().is_empty() {
                return Err(AppError::BadRequest("Name cannot be empty".to_string()));
            }
        }

        if let Some(code) = &req.code {
            if code.trim().is_empty() {
                return Err(AppError::BadRequest("Code cannot be empty".to_string()));
            }

            // Validate permission code against allowed patterns
            if let Err(e) = self.security_config.validate_permission_code(code) {
                return Err(AppError::BadRequest(e));
            }
        }

        self.permission_repo.update(id, req).await
    }

    pub async fn delete_permission(&self, id: i64) -> Result<(), AppError> {
        self.permission_repo.delete(id).await
    }
}
