use crate::{
    errors::AppError,
    models::entities::access_evaluation_log::{
        AccessDecision, AccessEvaluationLog, AccessEvaluationLogFilter, AccessEvaluationMetrics,
    },
};
use chrono::{DateTime, Utc};
use sqlx::{sqlite::SqlitePool, FromRow, Row};
use std::sync::Arc;
use tracing::{error, info};

/// Internal database representation of an access evaluation log
#[derive(FromRow)]
struct AccessEvaluationLogDb {
    id: i64,
    request_id: String,
    user_id: Option<i64>,
    resource_id: Option<String>,
    resource_type: Option<String>,
    action: String,
    decision: String,
    rbac_result: Option<String>,
    abac_result: Option<String>,
    applied_policies: Option<String>,
    evaluation_time_ms: Option<i64>,
    context_json: Option<String>,
    created_at: DateTime<Utc>,
}

impl TryFrom<AccessEvaluationLogDb> for AccessEvaluationLog {
    type Error = AppError;

    fn try_from(db: AccessEvaluationLogDb) -> Result<Self, Self::Error> {
        let decision = db
            .decision
            .parse::<AccessDecision>()
            .map_err(|e| AppError::InternalServerError(format!("Invalid decision: {}", e)))?;

        let rbac_result = if let Some(result) = db.rbac_result {
            Some(result.parse::<AccessDecision>().map_err(|e| {
                AppError::InternalServerError(format!("Invalid RBAC result: {}", e))
            })?)
        } else {
            None
        };

        let abac_result = if let Some(result) = db.abac_result {
            Some(result.parse::<AccessDecision>().map_err(|e| {
                AppError::InternalServerError(format!("Invalid ABAC result: {}", e))
            })?)
        } else {
            None
        };

        Ok(Self {
            id: db.id,
            request_id: db.request_id,
            user_id: db.user_id,
            resource_id: db.resource_id,
            resource_type: db.resource_type,
            action: db.action,
            decision,
            rbac_result,
            abac_result,
            applied_policies: db.applied_policies,
            evaluation_time_ms: db.evaluation_time_ms,
            context_json: db.context_json,
            created_at: db.created_at,
        })
    }
}

/// Repository for managing access evaluation logs
#[derive(Clone)]
pub struct AccessEvaluationLogRepository {
    pool: Arc<SqlitePool>,
}

impl AccessEvaluationLogRepository {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self { pool }
    }

    /// Creates a new access evaluation log entry
    pub async fn create(&self, log: &AccessEvaluationLog) -> Result<AccessEvaluationLog, AppError> {
        let result = sqlx::query(
            r#"
            INSERT INTO access_evaluation_log (
                request_id, user_id, resource_id, resource_type, action, 
                decision, rbac_result, abac_result, applied_policies, 
                evaluation_time_ms, context_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING *
            "#,
        )
        .bind(&log.request_id)
        .bind(log.user_id)
        .bind(&log.resource_id)
        .bind(&log.resource_type)
        .bind(&log.action)
        .bind(log.decision.to_string())
        .bind(log.rbac_result.as_ref().map(|r| r.to_string()))
        .bind(log.abac_result.as_ref().map(|r| r.to_string()))
        .bind(&log.applied_policies)
        .bind(log.evaluation_time_ms)
        .bind(&log.context_json)
        .map(|row: sqlx::sqlite::SqliteRow| AccessEvaluationLogDb {
            id: row.get("id"),
            request_id: row.get("request_id"),
            user_id: row.get("user_id"),
            resource_id: row.get("resource_id"),
            resource_type: row.get("resource_type"),
            action: row.get("action"),
            decision: row.get("decision"),
            rbac_result: row.get("rbac_result"),
            abac_result: row.get("abac_result"),
            applied_policies: row.get("applied_policies"),
            evaluation_time_ms: row.get("evaluation_time_ms"),
            context_json: row.get("context_json"),
            created_at: row.get("created_at"),
        })
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to create access evaluation log: {}", e);
            AppError::DatabaseError(e)
        })?;

        result.try_into()
    }

    /// Retrieves access evaluation logs with filtering and pagination
    pub async fn list(
        &self,
        filter: &AccessEvaluationLogFilter,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        let mut query = "SELECT * FROM access_evaluation_log WHERE 1=1".to_string();
        let mut conditions = Vec::new();

        if filter.user_id.is_some() {
            conditions.push("user_id = ?");
        }
        if filter.resource_type.is_some() {
            conditions.push("resource_type = ?");
        }
        if filter.action.is_some() {
            conditions.push("action = ?");
        }
        if filter.decision.is_some() {
            conditions.push("decision = ?");
        }
        if filter.start_date.is_some() {
            conditions.push("created_at >= ?");
        }
        if filter.end_date.is_some() {
            conditions.push("created_at <= ?");
        }

        if !conditions.is_empty() {
            query.push_str(" AND ");
            query.push_str(&conditions.join(" AND "));
        }

        query.push_str(" ORDER BY created_at DESC");

        if filter.limit.is_some() {
            query.push_str(" LIMIT ?");
        }
        if filter.offset.is_some() {
            query.push_str(" OFFSET ?");
        }

        let mut query_builder = sqlx::query_as::<_, AccessEvaluationLogDb>(&query);

        if let Some(user_id) = filter.user_id {
            query_builder = query_builder.bind(user_id);
        }
        if let Some(resource_type) = &filter.resource_type {
            query_builder = query_builder.bind(resource_type);
        }
        if let Some(action) = &filter.action {
            query_builder = query_builder.bind(action);
        }
        if let Some(decision) = &filter.decision {
            query_builder = query_builder.bind(decision.to_string());
        }
        if let Some(start_date) = filter.start_date {
            query_builder = query_builder.bind(start_date);
        }
        if let Some(end_date) = filter.end_date {
            query_builder = query_builder.bind(end_date);
        }
        if let Some(limit) = filter.limit {
            query_builder = query_builder.bind(limit);
        }
        if let Some(offset) = filter.offset {
            query_builder = query_builder.bind(offset);
        }

        let logs = query_builder
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| {
                error!("Failed to fetch access evaluation logs: {}", e);
                AppError::DatabaseError(e)
            })?
            .into_iter()
            .map(|db| db.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(logs)
    }

    /// Finds an access evaluation log by its ID
    pub async fn find_by_id(&self, id: i64) -> Result<Option<AccessEvaluationLog>, AppError> {
        let log = sqlx::query_as::<_, AccessEvaluationLogDb>(
            "SELECT * FROM access_evaluation_log WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to find access evaluation log by ID: {}", e);
            AppError::DatabaseError(e)
        })?;

        match log {
            Some(db) => Ok(Some(db.try_into()?)),
            None => Ok(None),
        }
    }

    /// Finds access evaluation logs by request ID
    pub async fn find_by_request_id(
        &self,
        request_id: &str,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        let logs = sqlx::query_as::<_, AccessEvaluationLogDb>(
            "SELECT * FROM access_evaluation_log WHERE request_id = ? ORDER BY created_at DESC",
        )
        .bind(request_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to find access evaluation logs by request ID: {}", e);
            AppError::DatabaseError(e)
        })?
        .into_iter()
        .map(|db| db.try_into())
        .collect::<Result<Vec<_>, _>>()?;

        Ok(logs)
    }

    /// Counts the total number of access evaluation logs matching the filter
    pub async fn count(&self, filter: &AccessEvaluationLogFilter) -> Result<i64, AppError> {
        let mut query = "SELECT COUNT(*) as count FROM access_evaluation_log WHERE 1=1".to_string();
        let mut conditions = Vec::new();

        if filter.user_id.is_some() {
            conditions.push("user_id = ?");
        }
        if filter.resource_type.is_some() {
            conditions.push("resource_type = ?");
        }
        if filter.action.is_some() {
            conditions.push("action = ?");
        }
        if filter.decision.is_some() {
            conditions.push("decision = ?");
        }
        if filter.start_date.is_some() {
            conditions.push("created_at >= ?");
        }
        if filter.end_date.is_some() {
            conditions.push("created_at <= ?");
        }

        if !conditions.is_empty() {
            query.push_str(" AND ");
            query.push_str(&conditions.join(" AND "));
        }

        let mut query_builder = sqlx::query(&query);

        if let Some(user_id) = filter.user_id {
            query_builder = query_builder.bind(user_id);
        }
        if let Some(resource_type) = &filter.resource_type {
            query_builder = query_builder.bind(resource_type);
        }
        if let Some(action) = &filter.action {
            query_builder = query_builder.bind(action);
        }
        if let Some(decision) = &filter.decision {
            query_builder = query_builder.bind(decision.to_string());
        }
        if let Some(start_date) = filter.start_date {
            query_builder = query_builder.bind(start_date);
        }
        if let Some(end_date) = filter.end_date {
            query_builder = query_builder.bind(end_date);
        }

        let count = query_builder
            .try_map(|row: sqlx::sqlite::SqliteRow| row.try_get::<i64, _>("count"))
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| {
                error!("Failed to count access evaluation logs: {}", e);
                AppError::DatabaseError(e)
            })?;

        Ok(count)
    }

    /// Gets performance metrics for access evaluations within a time period
    pub async fn get_metrics(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<AccessEvaluationMetrics, AppError> {
        let metrics = sqlx::query(
            r#"
            SELECT 
                COUNT(*) as total_evaluations,
                SUM(CASE WHEN decision = 'PERMIT' THEN 1 ELSE 0 END) as permit_count,
                SUM(CASE WHEN decision = 'DENY' THEN 1 ELSE 0 END) as deny_count,
                SUM(CASE WHEN decision = 'NOT_APPLICABLE' THEN 1 ELSE 0 END) as not_applicable_count,
                AVG(COALESCE(evaluation_time_ms, 0)) as avg_evaluation_time_ms,
                MAX(COALESCE(evaluation_time_ms, 0)) as max_evaluation_time_ms,
                MIN(COALESCE(evaluation_time_ms, 0)) as min_evaluation_time_ms,
                SUM(CASE WHEN rbac_result IS NOT NULL AND abac_result IS NULL THEN 1 ELSE 0 END) as rbac_only_count,
                SUM(CASE WHEN rbac_result IS NULL AND abac_result IS NOT NULL THEN 1 ELSE 0 END) as abac_only_count,
                SUM(CASE WHEN rbac_result IS NOT NULL AND abac_result IS NOT NULL THEN 1 ELSE 0 END) as hybrid_count
            FROM access_evaluation_log 
            WHERE created_at >= ? AND created_at <= ?
            "#,
        )
        .bind(start_date)
        .bind(end_date)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to get access evaluation metrics: {}", e);
            AppError::DatabaseError(e)
        })?;

        Ok(AccessEvaluationMetrics {
            total_evaluations: metrics.get("total_evaluations"),
            permit_count: metrics.get("permit_count"),
            deny_count: metrics.get("deny_count"),
            not_applicable_count: metrics.get("not_applicable_count"),
            average_evaluation_time_ms: metrics.get("avg_evaluation_time_ms"),
            max_evaluation_time_ms: metrics.get("max_evaluation_time_ms"),
            min_evaluation_time_ms: metrics.get("min_evaluation_time_ms"),
            rbac_only_count: metrics.get("rbac_only_count"),
            abac_only_count: metrics.get("abac_only_count"),
            hybrid_count: metrics.get("hybrid_count"),
            period_start: start_date,
            period_end: end_date,
        })
    }

    /// Deletes access evaluation logs older than the specified number of days
    pub async fn delete_older_than_days(&self, days: i64) -> Result<u64, AppError> {
        let threshold = Utc::now() - chrono::Duration::days(days);

        let result = sqlx::query("DELETE FROM access_evaluation_log WHERE created_at < ?")
            .bind(threshold)
            .execute(&*self.pool)
            .await
            .map_err(|e| {
                error!("Failed to delete old access evaluation logs: {}", e);
                AppError::DatabaseError(e)
            })?;

        info!(
            "Deleted {} old access evaluation log entries",
            result.rows_affected()
        );
        Ok(result.rows_affected())
    }

    /// Gets recent access evaluation logs for a specific user
    pub async fn get_recent_by_user(
        &self,
        user_id: i64,
        limit: i64,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        let logs = sqlx::query_as::<_, AccessEvaluationLogDb>(
            "SELECT * FROM access_evaluation_log WHERE user_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to fetch recent access evaluation logs for user: {}", e);
            AppError::DatabaseError(e)
        })?
        .into_iter()
        .map(|db| db.try_into())
        .collect::<Result<Vec<_>, _>>()?;

        Ok(logs)
    }

    /// Gets access evaluation logs for a specific resource
    pub async fn get_by_resource(
        &self,
        resource_id: &str,
        resource_type: &str,
        limit: Option<i64>,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        let limit = limit.unwrap_or(100);

        let logs = sqlx::query_as::<_, AccessEvaluationLogDb>(
            "SELECT * FROM access_evaluation_log WHERE resource_id = ? AND resource_type = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(resource_id)
        .bind(resource_type)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to fetch access evaluation logs for resource: {}", e);
            AppError::DatabaseError(e)
        })?
        .into_iter()
        .map(|db| db.try_into())
        .collect::<Result<Vec<_>, _>>()?;

        Ok(logs)
    }

    /// Gets access evaluation logs by policy usage
    pub async fn get_by_policy_usage(
        &self,
        policy_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        let limit = limit.unwrap_or(100);

        let logs = sqlx::query_as::<_, AccessEvaluationLogDb>(
            "SELECT * FROM access_evaluation_log WHERE applied_policies LIKE ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(format!("%{}%", policy_id))
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            error!("Failed to fetch access evaluation logs by policy usage: {}", e);
            AppError::DatabaseError(e)
        })?
        .into_iter()
        .map(|db| db.try_into())
        .collect::<Result<Vec<_>, _>>()?;

        Ok(logs)
    }

    /// Gets evaluation statistics by decision type
    pub async fn get_decision_statistics(
        &self,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
    ) -> Result<std::collections::HashMap<String, i64>, AppError> {
        let mut query_str = r#"
            SELECT decision, COUNT(*) as count 
            FROM access_evaluation_log 
            WHERE 1=1
        "#
        .to_string();

        let mut conditions = Vec::new();
        if start_date.is_some() {
            conditions.push(" AND created_at >= ?");
        }
        if end_date.is_some() {
            conditions.push(" AND created_at <= ?");
        }

        for condition in conditions {
            query_str.push_str(condition);
        }
        query_str.push_str(" GROUP BY decision");

        let mut query_builder = sqlx::query(&query_str);

        if let Some(start) = start_date {
            query_builder = query_builder.bind(start);
        }

        if let Some(end) = end_date {
            query_builder = query_builder.bind(end);
        }

        let rows = query_builder.fetch_all(&*self.pool).await.map_err(|e| {
            error!("Failed to get decision statistics: {}", e);
            AppError::DatabaseError(e)
        })?;

        let mut stats = std::collections::HashMap::new();
        for row in rows {
            let decision: String = row.get("decision");
            let count: i64 = row.get("count");
            stats.insert(decision, count);
        }

        Ok(stats)
    }

    /// Bulk create access evaluation logs (for batch operations)
    pub async fn bulk_create(&self, logs: Vec<AccessEvaluationLog>) -> Result<Vec<i64>, AppError> {
        let mut tx = self.pool.begin().await?;
        let mut created_ids = Vec::new();

        for log in logs {
            let result = sqlx::query(
                r#"
                INSERT INTO access_evaluation_log (
                    request_id, user_id, resource_id, resource_type, action, 
                    decision, rbac_result, abac_result, applied_policies, 
                    evaluation_time_ms, context_json
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                RETURNING id
                "#,
            )
            .bind(&log.request_id)
            .bind(log.user_id)
            .bind(&log.resource_id)
            .bind(&log.resource_type)
            .bind(&log.action)
            .bind(log.decision.to_string())
            .bind(log.rbac_result.as_ref().map(|r| r.to_string()))
            .bind(log.abac_result.as_ref().map(|r| r.to_string()))
            .bind(&log.applied_policies)
            .bind(log.evaluation_time_ms)
            .bind(&log.context_json)
            .fetch_one(&mut *tx)
            .await?;

            let id: i64 = result.get("id");
            created_ids.push(id);
        }

        tx.commit().await?;
        Ok(created_ids)
    }

    /// List logs with filters for API
    pub async fn list_with_filters(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
        result: Option<&str>,
        evaluation_type: Option<&str>,
        date_range: Option<&str>,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        let mut query = String::from(
            "SELECT id, request_id, user_id, resource_id, resource_type, action, decision,
             rbac_result, abac_result, applied_policies, evaluation_time_ms, context_json, created_at
             FROM access_evaluation_log WHERE 1=1"
        );

        if search.is_some() {
            query.push_str(" AND (resource_type LIKE '%' || ? || '%' OR action LIKE '%' || ? || '%')");
        }
        if result.is_some() {
            query.push_str(" AND UPPER(decision) = UPPER(?)");
        }
        if let Some(eval_type) = evaluation_type {
            match eval_type.to_lowercase().as_str() {
                "rbac" => query.push_str(" AND rbac_result IS NOT NULL AND abac_result IS NULL"),
                "abac" => query.push_str(" AND abac_result IS NOT NULL AND rbac_result IS NULL"),
                "hybrid" => query.push_str(" AND rbac_result IS NOT NULL AND abac_result IS NOT NULL"),
                _ => {}
            }
        }
        if let Some(range) = date_range {
            match range {
                "today" => query.push_str(" AND DATE(created_at) = DATE('now')"),
                "week" => query.push_str(" AND created_at >= datetime('now', '-7 days')"),
                "month" => query.push_str(" AND created_at >= datetime('now', '-30 days')"),
                _ => {}
            }
        }

        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

        let mut query_builder = sqlx::query_as::<_, AccessEvaluationLogDb>(&query);

        if let Some(s) = search {
            query_builder = query_builder.bind(s).bind(s);
        }
        if let Some(r) = result {
            query_builder = query_builder.bind(r);
        }

        query_builder = query_builder.bind(limit).bind(offset);

        let logs = query_builder
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| {
                error!("Failed to fetch logs: {}", e);
                AppError::DatabaseError(e)
            })?
            .into_iter()
            .map(|db| db.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(logs)
    }

    /// Count logs with filters
    pub async fn count_with_filters(
        &self,
        search: Option<&str>,
        result: Option<&str>,
        evaluation_type: Option<&str>,
        date_range: Option<&str>,
    ) -> Result<i64, AppError> {
        let mut query = String::from("SELECT COUNT(*) FROM access_evaluation_log WHERE 1=1");

        if search.is_some() {
            query.push_str(" AND (resource_type LIKE '%' || ? || '%' OR action LIKE '%' || ? || '%')");
        }
        if result.is_some() {
            query.push_str(" AND UPPER(decision) = UPPER(?)");
        }
        if let Some(eval_type) = evaluation_type {
            match eval_type.to_lowercase().as_str() {
                "rbac" => query.push_str(" AND rbac_result IS NOT NULL AND abac_result IS NULL"),
                "abac" => query.push_str(" AND abac_result IS NOT NULL AND rbac_result IS NULL"),
                "hybrid" => query.push_str(" AND rbac_result IS NOT NULL AND abac_result IS NOT NULL"),
                _ => {}
            }
        }
        if let Some(range) = date_range {
            match range {
                "today" => query.push_str(" AND DATE(created_at) = DATE('now')"),
                "week" => query.push_str(" AND created_at >= datetime('now', '-7 days')"),
                "month" => query.push_str(" AND created_at >= datetime('now', '-30 days')"),
                _ => {}
            }
        }

        let mut query_builder = sqlx::query_scalar::<_, i64>(&query);

        if let Some(s) = search {
            query_builder = query_builder.bind(s).bind(s);
        }
        if let Some(r) = result {
            query_builder = query_builder.bind(r);
        }

        let count = query_builder.fetch_one(&*self.pool).await.map_err(|e| {
            error!("Failed to count logs: {}", e);
            AppError::DatabaseError(e)
        })?;

        Ok(count)
    }
}
