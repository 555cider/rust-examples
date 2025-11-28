use super::*;

impl RbacEngine {
    /// Get user with resolved roles and permissions
    pub(crate) async fn get_user_with_roles(
        &self,
        user_id: i64,
    ) -> AccessControlResult<UserWithRoles> {
        // Check cache first
        if self.config.enable_caching {
            if let Some(cache) = &self.cache {
                if let Some(permissions) = cache.get_user_permissions(user_id).await {
                    // Get user and role separately for cache hit
                    let user_response = self.user_repo.find_by_id(user_id).await.map_err(|e| {
                        AccessControlError::RbacError {
                            message: format!("Failed to get user: {}", e),
                            source: Some(Box::new(e)),
                            context: ErrorContext::new("get_user_with_roles").with_user(user_id),
                        }
                    })?;
                    let role_response = self
                        .role_repo
                        .find_by_id(user_response.role_id)
                        .await
                        .map_err(|e| AccessControlError::RbacError {
                            message: format!("Failed to get user type: {}", e),
                            source: Some(Box::new(e)),
                            context: ErrorContext::new("get_user_with_roles").with_user(user_id),
                        })?;

                    let user = AdminUser {
                        id: user_response.id,
                        username: user_response.username.clone(),
                        password_hash: String::new(),
                        email: user_response.email.clone(),
                        role_id: user_response.role_id,
                        is_active: user_response.is_active,
                        attributes: user_response.attributes.clone(),
                        last_login_at: user_response.last_login_at.map(|dt| dt.naive_utc()),
                        created_at: user_response.created_at.naive_utc(),
                        updated_at: user_response.updated_at.naive_utc(),
                    };

                    let role = RoleEntity {
                        id: role_response.id,
                        code: role_response.code.clone(),
                        name: role_response.name.clone(),
                        description: role_response.description.clone(),
                        is_active: role_response.is_active,
                        created_at: chrono::Utc::now().naive_utc(),
                        updated_at: chrono::Utc::now().naive_utc(),
                    };

                    let role_name = role.name.clone();
                    return Ok(UserWithRoles {
                        user,
                        role,
                        effective_permissions: permissions,
                        role_hierarchy: vec![role_name],
                    });
                }
            }
        }

        // Get user from database
        let user_response = self.user_repo.find_by_id(user_id).await.map_err(|e| {
            AccessControlError::RbacError {
                message: format!("Failed to get user: {}", e),
                source: Some(Box::new(e)),
                context: ErrorContext::new("get_user_with_roles").with_user(user_id),
            }
        })?;

        // Convert UserResponse to AdminUser
        let user = AdminUser {
            id: user_response.id,
            username: user_response.username.clone(),
            password_hash: String::new(), // Not needed for access control
            email: user_response.email.clone(),
            role_id: user_response.role_id,
            is_active: user_response.is_active,
            attributes: user_response.attributes.clone(),
            last_login_at: user_response.last_login_at.map(|dt| dt.naive_utc()),
            created_at: user_response.created_at.naive_utc(),
            updated_at: user_response.updated_at.naive_utc(),
        };

        // Get user type (role)
        let role_response = self
            .role_repo
            .find_by_id(user.role_id)
            .await
            .map_err(|e| AccessControlError::RbacError {
                message: format!("Failed to get user type: {}", e),
                source: Some(Box::new(e)),
                context: ErrorContext::new("get_user_with_roles")
                    .with_user(user_id)
                    .with_data("role_id", user.role_id),
            })?;

        let role = RoleEntity {
            id: role_response.id,
            code: role_response.code.clone(),
            name: role_response.name.clone(),
            description: role_response.description.clone(),
            is_active: role_response.is_active,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
        };

        // Get permissions for the user type
        let permissions = self
            .get_permissions_for_role(user.role_id)
            .await?;

        // Cache the permissions
        if self.config.enable_caching {
            if let Some(cache) = &self.cache {
                cache.cache_user_permissions(user_id, &permissions).await;
            }
        }

        Ok(UserWithRoles {
            user,
            role: role.clone(),
            effective_permissions: permissions,
            role_hierarchy: vec![role.name],
        })
    }

    /// Get permissions for a user type (role) using optimized database query
    /// This method uses the optimized repository method to avoid N+1 queries
    /// and eliminates in-memory filtering
    pub(super) async fn get_permissions_for_role(
        &self,
        role_id: i64,
    ) -> AccessControlResult<Vec<Permission>> {
        // Use optimized query that performs JOIN at database level
        let permissions = self
            .permission_repo
            .find_by_role_id_optimized(role_id)
            .await
            .map_err(|e| AccessControlError::RbacError {
                message: format!("Failed to get permissions for user type: {}", e),
                source: Some(Box::new(e)),
                context: ErrorContext::new("get_permissions_for_role")
                    .with_data("role_id", role_id),
            })?;

        debug!(
            role_id = role_id,
            permission_count = permissions.len(),
            "Retrieved permissions for user type using optimized query"
        );

        Ok(permissions)
    }
}
