use crate::errors::AppError;
use crate::models::entities::abac::Policy;
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::{debug, error};

#[derive(Clone)]
pub struct PolicyRepository {
    pool: Arc<SqlitePool>,
}

impl PolicyRepository {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self { pool }
    }

    /// Validate policy JSON structure
    pub async fn validate_policy_json(&self, policy: &Policy) -> Result<(), AppError> {
        // Validate target JSON
        policy.parse_target().map_err(|e| {
            AppError::BadRequest(format!("Invalid target JSON: {}", e))
        })?;

        // Validate rules JSON
        policy.parse_rules().map_err(|e| {
            AppError::BadRequest(format!("Invalid rules JSON: {}", e))
        })?;

        Ok(())
    }

    /// Create a new policy
    pub async fn create(&self, policy: Policy) -> Result<String, AppError> {
        debug!("Creating policy: {}", policy.id);

        sqlx::query(
            "INSERT INTO abac_policy (id, name, description, target_json, rules_json, effect, priority, is_active, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&policy.id)
        .bind(&policy.name)
        .bind(&policy.description)
        .bind(&policy.target_json)
        .bind(&policy.rules_json)
        .bind(policy.effect.to_string())
        .bind(policy.priority)
        .bind(policy.is_active)
        .bind(policy.created_at)
        .bind(policy.updated_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to create policy: {}", e);
            AppError::DatabaseError(e)
        })?;

        Ok(policy.id)
    }

    /// Find all policies with optional filters
    pub async fn find_all(
        &self,
        search: Option<String>,
        is_active: Option<bool>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Policy>, AppError> {
        let mut query_builder = sqlx::QueryBuilder::new(
            "SELECT id, name, description, target_json, rules_json, effect, priority, is_active, created_at, updated_at
             FROM abac_policy WHERE 1=1"
        );

        let search_term = search.map(|s| format!("%{}%", s));
        if let Some(ref term) = search_term {
            query_builder.push(" AND (name LIKE ");
            query_builder.push_bind(term);
            query_builder.push(" OR description LIKE ");
            query_builder.push_bind(term);
            query_builder.push(")");
        }

        if let Some(active) = is_active {
            query_builder.push(" AND is_active = ");
            query_builder.push_bind(if active { 1 } else { 0 });
        }

        query_builder.push(" ORDER BY priority DESC, created_at DESC");

        if let Some(lim) = limit {
            query_builder.push(" LIMIT ");
            query_builder.push_bind(lim);
        }

        if let Some(off) = offset {
            query_builder.push(" OFFSET ");
            query_builder.push_bind(off);
        }

        let policies = query_builder
            .build_query_as::<Policy>()
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| {
                error!("Failed to fetch policies: {}", e);
                AppError::DatabaseError(e)
            })?;

        Ok(policies)
    }

    /// Find a policy by ID
    pub async fn find_by_id(&self, id: &str) -> Result<Policy, AppError> {
        debug!("Finding policy by ID: {}", id);

        let policy = sqlx::query_as::<_, Policy>(
            "SELECT id, name, description, target_json, rules_json, effect, priority, is_active, created_at, updated_at 
             FROM abac_policy WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to fetch policy: {}", e);
            AppError::DatabaseError(e)
        })?
        .ok_or_else(|| AppError::NotFound(format!("Policy with id {} not found", id)))?;

        Ok(policy)
    }

    /// Update a policy
    pub async fn update(&self, id: &str, policy: Policy) -> Result<Policy, AppError> {
        debug!("Updating policy: {}", id);

        sqlx::query(
            "UPDATE abac_policy 
             SET name = ?, description = ?, target_json = ?, rules_json = ?, effect = ?, priority = ?, is_active = ?, updated_at = ?
             WHERE id = ?"
        )
        .bind(&policy.name)
        .bind(&policy.description)
        .bind(&policy.target_json)
        .bind(&policy.rules_json)
        .bind(policy.effect.to_string())
        .bind(policy.priority)
        .bind(policy.is_active)
        .bind(chrono::Utc::now().naive_utc())
        .bind(id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to update policy: {}", e);
            AppError::DatabaseError(e)
        })?;

        self.find_by_id(id).await
    }

    /// Soft delete a policy (set is_active to false)
    pub async fn soft_delete(&self, id: &str) -> Result<(), AppError> {
        debug!("Soft deleting policy: {}", id);

        let result = sqlx::query(
            "UPDATE abac_policy SET is_active = 0, updated_at = ? WHERE id = ?"
        )
        .bind(chrono::Utc::now().naive_utc())
        .bind(id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to soft delete policy: {}", e);
            AppError::DatabaseError(e)
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Policy with id {} not found", id)));
        }

        Ok(())
    }

    /// Hard delete a policy
    pub async fn delete(&self, id: &str) -> Result<(), AppError> {
        debug!("Deleting policy: {}", id);

        let result = sqlx::query("DELETE FROM abac_policy WHERE id = ?")
            .bind(id)
            .execute(&*self.pool)
            .await
            .map_err(|e| {
                error!("Failed to delete policy: {}", e);
                AppError::DatabaseError(e)
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Policy with id {} not found", id)));
        }

        Ok(())
    }

    /// Count total policies
    pub async fn count(&self, search: Option<String>, is_active: Option<bool>) -> Result<i64, AppError> {
        let mut query_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM abac_policy WHERE 1=1");

        let search_term = search.map(|s| format!("%{}%", s));
        if let Some(ref term) = search_term {
            query_builder.push(" AND (name LIKE ");
            query_builder.push_bind(term);
            query_builder.push(" OR description LIKE ");
            query_builder.push_bind(term);
            query_builder.push(")");
        }

        if let Some(active) = is_active {
            query_builder.push(" AND is_active = ");
            query_builder.push_bind(if active { 1 } else { 0 });
        }

        let count: (i64,) = query_builder
            .build_query_as()
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| {
                error!("Failed to count policies: {}", e);
                AppError::DatabaseError(e)
            })?;

        Ok(count.0)
    }

    /// Get active policies for evaluation
    pub async fn get_active_policies(&self) -> Result<Vec<Policy>, AppError> {
        self.find_all(None, Some(true), None, None).await
    }

    /// Find active policies (alias for compatibility)
    pub async fn find_active_policies(&self) -> Result<Vec<Policy>, AppError> {
        self.get_active_policies().await
    }
}
