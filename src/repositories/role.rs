use crate::{
    errors::AppError,
    models::{
        dto::{
            common::ListQueryParams,
            role::{CreateRoleRequest, UpdateRoleRequest, RoleResponse},
        },
        entities::role::Role,
    },
};
// Remove unused imports
use sqlx::{sqlite::SqliteArguments, Arguments, SqlitePool};
use std::sync::Arc;

#[derive(Clone)]
pub struct RoleRepository {
    pool: Arc<SqlitePool>,
}

impl RoleRepository {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self { pool }
    }

    pub async fn create(&self, req: CreateRoleRequest) -> Result<RoleResponse, AppError> {
        // First check if a user type with this code already exists
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM roles WHERE code = ?")
            .bind(&req.code)
            .fetch_optional(&*self.pool)
            .await?;

        if exists.is_some() {
            return Err(AppError::Conflict(
                "User type with this code already exists".to_string(),
            ));
        }

        // Start transaction
        let mut tx = self.pool.begin().await?;

        // Insert the new user type
        let result = sqlx::query!(
            r#"
            INSERT INTO roles (code, name, description, is_active)
            VALUES (?, ?, ?, ?)
            "#,
            req.code,
            req.name,
            req.description,
            req.is_active
        )
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::InternalServerError(
                "Failed to create user type".to_string(),
            ));
        }

        // Get the ID of the newly created user type
        let id = sqlx::query_scalar::<_, i64>("SELECT last_insert_rowid()")
            .fetch_one(&mut *tx)
            .await?;

        // Insert permissions if provided
        if let Some(permissions) = &req.permissions {
            for permission_id in permissions {
                sqlx::query!(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
                    id,
                    permission_id
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;

        // Return the full user type
        self.find_by_id(id).await
    }

    pub async fn find_all(
        &self,
        query: ListQueryParams,
    ) -> Result<Vec<RoleResponse>, AppError> {
        let limit = query.get_limit();
        let offset = query.get_offset();
        let allowed_sort_columns = [
            "id",
            "code",
            "name",
            "is_active",
            "created_at",
            "updated_at",
        ];
        let order_by = query.get_order_by(&allowed_sort_columns);

        let base_query = "SELECT * FROM roles WHERE is_active = true";

        let (query_str, params) = if let Some(search) = &query.q {
            let search_term = format!("%{}%", search);
            let query_str = format!(
                "{} AND (code LIKE ? OR name LIKE ? OR description LIKE ?) ORDER BY {} LIMIT ? OFFSET ?",
                base_query, order_by
            );
            (
                query_str,
                vec![
                    search_term.clone(),
                    search_term.clone(),
                    search_term,
                    limit.to_string(),
                    offset.to_string(),
                ],
            )
        } else {
            let query_str = format!("{} ORDER BY {} LIMIT ? OFFSET ?", base_query, order_by);
            (query_str, vec![limit.to_string(), offset.to_string()])
        };

        let mut query_builder = sqlx::query_as::<_, Role>(&query_str);

        // Add parameters to the query
        for param in params {
            query_builder = query_builder.bind(param);
        }

        let roles = query_builder
            .fetch_all(&*self.pool)
            .await?
            .into_iter()
            .map(RoleResponse::from)
            .collect();

        Ok(roles)
    }

    pub async fn find_by_id(&self, type_id: i64) -> Result<RoleResponse, AppError> {
        let role = sqlx::query_as::<_, Role>(
            "SELECT id, code, name, description, is_active, created_at, updated_at FROM roles WHERE id = ?",
        )
        .bind(type_id)
        .fetch_optional(&*self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User type not found".to_string()))?;

        let mut response = RoleResponse::from(role);

        // Fetch permissions
        let permissions = sqlx::query_scalar::<_, i64>(
            "SELECT permission_id FROM role_permissions WHERE role_id = ?",
        )
        .bind(type_id)
        .fetch_all(&*self.pool)
        .await?;

        response.permissions = Some(permissions);

        Ok(response)
    }

    pub async fn update(
        &self,
        type_id: i64,
        req: UpdateRoleRequest,
    ) -> Result<RoleResponse, AppError> {
        let mut updates = Vec::new();
        let mut params = SqliteArguments::default();

        if let Some(code) = req.code {
            updates.push("code = ?");
            let _ = params.add(code);
        }

        if let Some(name) = req.name {
            updates.push("name = ?");
            let _ = params.add(name);
        }

        if let Some(description) = req.description {
            updates.push("description = ?");
            let _ = params.add(description);
        }

        if let Some(is_active) = req.is_active {
            updates.push("is_active = ?");
            let _ = params.add(is_active);
        }

        // Start transaction
        let mut tx = self.pool.begin().await?;

        if !updates.is_empty() {
            updates.push("updated_at = CURRENT_TIMESTAMP");
            let set_clause = updates.join(", ");
            let query_str = format!("UPDATE roles SET {} WHERE id = ?", set_clause);
            let _ = params.add(type_id);

            sqlx::query_with(&query_str, params)
                .execute(&mut *tx)
                .await?;
        }

        // Update permissions if provided
        if let Some(permissions) = req.permissions {
            // Delete existing permissions
            sqlx::query!("DELETE FROM role_permissions WHERE role_id = ?", type_id)
                .execute(&mut *tx)
                .await?;

            // Insert new permissions
            for permission_id in permissions {
                sqlx::query!(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
                    type_id,
                    permission_id
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;

        self.find_by_id(type_id).await
    }

    pub async fn delete(&self, type_id: i64) -> Result<(), AppError> {
        // Check if there are any active users with this user type
        let user_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_user WHERE role_id = ? AND is_active = true",
        )
        .bind(type_id)
        .fetch_one(&*self.pool)
        .await?;

        if user_count > 0 {
            return Err(AppError::BadRequest(
                "Cannot deactivate user type that is in use by active users".to_string(),
            ));
        }

        // Soft delete by setting is_active to false
        let result = sqlx::query!(
            "UPDATE roles SET is_active = 0, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            type_id
        )
        .execute(&*self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound("User type not found".to_string()));
        }

        Ok(())
    }



    pub async fn find_by_code(&self, code: &str) -> Result<Option<RoleResponse>, AppError> {
        let role = sqlx::query_as::<_, Role>(
            "SELECT id, code, name, description, is_active, created_at, updated_at FROM roles WHERE code = ?",
        )
        .bind(code)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(role.map(RoleResponse::from))
    }

    pub async fn count_all(&self, query: &ListQueryParams) -> Result<i64, AppError> {
        let base_query = "SELECT COUNT(*) FROM roles WHERE is_active = true";

        let count = if let Some(search) = &query.q {
            let search_term = format!("%{}%", search);
            sqlx::query_scalar::<_, i64>(&format!(
                "{} AND (code LIKE ? OR name LIKE ? OR description LIKE ?)",
                base_query
            ))
            .bind(&search_term)
            .bind(&search_term)
            .bind(&search_term)
            .fetch_one(&*self.pool)
            .await?
        } else {
            sqlx::query_scalar::<_, i64>(base_query)
                .fetch_one(&*self.pool)
                .await?
        };

        Ok(count)
    }
}
