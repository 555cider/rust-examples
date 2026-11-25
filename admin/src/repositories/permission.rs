use crate::{
    errors::AppError,
    models::{
        dto::common::ListQueryParams, dto::permission::CreatePermissionRequest,
        dto::permission::PermissionResponse, dto::permission::UpdatePermissionRequest,
        entities::permission::Permission,
    },
    services::access_control::cache::{CacheInvalidationManager, InvalidationEvent},
};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tracing::{debug, warn};

#[derive(Clone)]
pub struct PermissionRepository {
    pool: Arc<SqlitePool>,
    cache_invalidation: Option<Arc<CacheInvalidationManager>>,
}

impl PermissionRepository {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
            cache_invalidation: None,
        }
    }

    /// Create repository with cache invalidation support
    pub fn with_cache_invalidation(
        pool: Arc<SqlitePool>,
        cache_invalidation: Arc<CacheInvalidationManager>,
    ) -> Self {
        Self {
            pool,
            cache_invalidation: Some(cache_invalidation),
        }
    }

    /// Trigger cache invalidation for permission changes
    async fn invalidate_permission_cache(&self, permission_id: i64, event_type: &str) {
        if let Some(invalidation_manager) = &self.cache_invalidation {
            let event = match event_type {
                "created" => InvalidationEvent::PermissionCreated {
                    permission_id: permission_id.to_string(),
                },
                "updated" => InvalidationEvent::PermissionUpdated {
                    permission_id: permission_id.to_string(),
                },
                "deleted" => InvalidationEvent::PermissionDeleted {
                    permission_id: permission_id.to_string(),
                },
                _ => return,
            };

            if let Err(e) = invalidation_manager.invalidate(event).await {
                warn!(
                    "Failed to invalidate cache for permission {}: {}",
                    permission_id, e
                );
            } else {
                debug!(
                    "Cache invalidated for permission {} ({})",
                    permission_id, event_type
                );
            }
        }
    }

    pub async fn create(&self, req: CreatePermissionRequest) -> Result<i64, AppError> {
        let permission = req.to_permission();

        let inserted_id = sqlx::query!(
            "INSERT INTO permission (code, name, description, category) VALUES (?, ?, ?, ?) RETURNING id",
            permission.code,
            permission.name,
            permission.description,
            permission.category
        )
        .fetch_one(&*self.pool)
        .await?
        .id;

        // Trigger cache invalidation
        self.invalidate_permission_cache(inserted_id, "created")
            .await;

        Ok(inserted_id)
    }

    pub async fn find_all(
        &self,
        query: ListQueryParams,
    ) -> Result<Vec<PermissionResponse>, AppError> {
        let limit = query.get_limit();
        let offset = query.get_offset();

        let allowed_sort_columns = [
            "id",
            "code",
            "name",
            "category",
            "created_at",
            "updated_at",
        ];
        let order_by = query.get_order_by(&allowed_sort_columns);

        let query_str = format!(
            "SELECT id, code, name, description, category, created_at, updated_at FROM permission ORDER BY {} LIMIT ? OFFSET ?",
            order_by
        );

        let permissions = sqlx::query_as::<_, Permission>(&query_str)
            .bind(limit)
            .bind(offset)
            .fetch_all(&*self.pool)
            .await?;

        Ok(permissions
            .into_iter()
            .map(PermissionResponse::from)
            .collect())
    }

    pub async fn find_by_id(&self, id: i64) -> Result<PermissionResponse, AppError> {
        let result = sqlx::query_as!(
            Permission,
            "SELECT id, code, name, description, category, created_at, updated_at FROM permission WHERE id = ?",
            id
        )
        .fetch_optional(&*self.pool)
        .await?;

        match result {
            Some(permission) => Ok(permission.into()),
            None => {
                tracing::warn!("Permission not found with ID: {}", id);
                Err(AppError::NotFound(format!(
                    "Permission with ID {} not found",
                    id
                )))
            }
        }
    }

    pub async fn find_by_role_id(
        &self,
        role_id: i64,
    ) -> Result<Vec<Permission>, AppError> {
        let permissions = sqlx::query_as!(
            Permission,
            "SELECT p.id, p.code, p.name, p.description, p.category, p.created_at, p.updated_at FROM permission p INNER JOIN role_permissions utp ON p.id = utp.permission_id WHERE utp.role_id = ?",
            role_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(permissions)
    }

    /// Optimized query to fetch permissions for a user type with database-level filtering
    /// This method uses JOIN and WHERE clauses to avoid N+1 queries
    pub async fn find_by_role_id_optimized(
        &self,
        role_id: i64,
    ) -> Result<Vec<Permission>, AppError> {
        let permissions = sqlx::query_as!(
            Permission,
            r#"
            SELECT p.id, p.code, p.name, p.description, p.category, 
                   p.created_at, p.updated_at
            FROM permission p
            INNER JOIN role_permissions utp ON p.id = utp.permission_id
            WHERE utp.role_id = ?
            ORDER BY p.category, p.code
            "#,
            role_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(permissions)
    }

    /// Batch fetch permissions for multiple users to avoid N+1 queries
    /// Returns a HashMap mapping user_id to their permissions
    pub async fn find_by_user_ids_batch(
        &self,
        user_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<Permission>>, AppError> {
        use std::collections::HashMap;

        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Build placeholders for the IN clause
        let placeholders = user_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

        let query = format!(
            r#"
            SELECT u.id as user_id, 
                   p.id, p.code, p.name, p.description, p.category,
                   p.created_at, p.updated_at
            FROM admin_user u
            INNER JOIN role_permissions utp ON u.role_id = utp.role_id
            INNER JOIN permission p ON utp.permission_id = p.id
            WHERE u.id IN ({})
            ORDER BY u.id, p.category, p.code
            "#,
            placeholders
        );

        // Build and execute query with parameter binding
        let mut query_builder = sqlx::query(&query);
        for user_id in user_ids {
            query_builder = query_builder.bind(user_id);
        }

        let rows = query_builder.fetch_all(&*self.pool).await?;

        // Group permissions by user_id
        let mut result: HashMap<i64, Vec<Permission>> = HashMap::new();

        for row in rows {
            let user_id: i64 = row.get("user_id");

            let permission = Permission {
                id: row.get("id"),
                code: row.get("code"),
                name: row.get("name"),
                description: row.get("description"),
                category: row.get("category"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            };

            result
                .entry(user_id)
                .or_insert_with(Vec::new)
                .push(permission);
        }

        Ok(result)
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<PermissionResponse>, AppError> {
        let result = sqlx::query_as!(
            Permission,
            "SELECT id, code, name, description, category, created_at, updated_at FROM permission WHERE name = ?",
            name
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(PermissionResponse::from))
    }

    pub async fn update(
        &self,
        id: i64,
        req: UpdatePermissionRequest,
    ) -> Result<PermissionResponse, AppError> {
        // First, get the current permission to check if it exists
        let _ = self.find_by_id(id).await?;

        // Start building the query
        let mut query = "UPDATE permission SET ".to_string();
        let mut updates = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(code) = &req.code {
            updates.push("code = ?");
            params.push(code.clone());
        }

        if let Some(name) = &req.name {
            updates.push("name = ?");
            params.push(name.clone());
        }

        if let Some(description) = &req.description {
            updates.push("description = ?");
            params.push(description.clone());
        }

        if let Some(category) = &req.category {
            updates.push("category = ?");
            params.push(category.clone());
        }

        if updates.is_empty() {
            return self.find_by_id(id).await;
        }

        updates.push("updated_at = CURRENT_TIMESTAMP");
        query.push_str(&updates.join(", "));
        query.push_str(" WHERE id = ?");

        // Execute the query with parameters
        let mut query_builder = sqlx::query(&query);

        // Bind parameters in order
        for param in params {
            query_builder = query_builder.bind(param);
        }

        // Bind the id parameter
        query_builder = query_builder.bind(id);

        // Execute the query
        query_builder.execute(&*self.pool).await?;

        // Trigger cache invalidation
        self.invalidate_permission_cache(id, "updated").await;

        // Return the updated permission
        self.find_by_id(id).await
    }

    pub async fn count(&self) -> Result<i64, AppError> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) as count FROM permission
            "#
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(result)
    }

    pub async fn delete(&self, id: i64) -> Result<(), AppError> {
        // First check if permission exists
        let _ = self.find_by_id(id).await?;

        // Delete permission (this will cascade delete role_permissions entries if foreign key is set)
        let result = sqlx::query!("DELETE FROM permission WHERE id = ?", id)
            .execute(&*self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Permission with ID {} not found",
                id
            )));
        }

        // Trigger cache invalidation
        self.invalidate_permission_cache(id, "deleted").await;

        Ok(())
    }
}
