use crate::{
    errors::AppError,
    models::dto::{
        common::ListQueryParams,
        role::{CreateRoleRequest, UpdateRoleRequest, RoleResponse},
    },
    repositories::role::RoleRepository,
};
use std::sync::Arc;
use validator::Validate;

#[derive(Clone)]
pub struct RoleService {
    role_repo: Arc<RoleRepository>,
}

impl RoleService {
    pub fn new(role_repo: RoleRepository) -> Self {
        Self {
            role_repo: Arc::new(role_repo),
        }
    }

    pub async fn create_role(
        &self,
        req: CreateRoleRequest,
    ) -> Result<RoleResponse, AppError> {
        req.validate()?;

        // Check if code already exists
        let exists = self.role_repo.find_by_code(&req.code).await?;
        if exists.is_some() {
            return Err(AppError::Conflict(
                "Role with this code already exists".to_string(),
            ));
        }

        self.role_repo.create(req).await
    }

    pub async fn get_roles(
        &self,
        query: ListQueryParams,
    ) -> Result<Vec<RoleResponse>, AppError> {
        self.role_repo.find_all(query).await
    }

    pub async fn get_role_by_id(&self, type_id: i64) -> Result<RoleResponse, AppError> {
        self.role_repo.find_by_id(type_id).await
    }

    pub async fn update_role(
        &self,
        type_id: i64,
        req: UpdateRoleRequest,
    ) -> Result<RoleResponse, AppError> {
        req.validate()?;

        // If code is being updated, check if it already exists
        if let Some(code) = &req.code {
            if let Some(existing) = self.role_repo.find_by_code(code).await? {
                if existing.id != type_id {
                    return Err(AppError::Conflict(
                        "Role with this code already exists".to_string(),
                    ));
                }
            }
        }

        self.role_repo.update(type_id, req).await
    }

    pub async fn delete_role(&self, type_id: i64) -> Result<(), AppError> {
        self.role_repo.delete(type_id).await
    }

    pub async fn count_roles(&self, query: &ListQueryParams) -> Result<i64, AppError> {
        self.role_repo.count_all(query).await
    }
}
