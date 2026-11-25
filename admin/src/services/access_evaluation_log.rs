use crate::{
    errors::AppError,
    models::entities::access_evaluation_log::{
        AccessDecision, AccessEvaluationLog, AccessEvaluationLogFilter, AccessEvaluationMetrics,
    },
    repositories::AccessEvaluationLogRepository,
    utils::log_redaction::LogRedactor,
};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::Value;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub total: i64,
    pub allowed: i64,
    pub denied: i64,
    pub abac_count: i64,
    pub rbac_count: i64,
    pub hybrid_count: i64,
}

/// Service for managing access evaluation logs and performance metrics
#[derive(Clone)]
pub struct AccessEvaluationLogService {
    log_repo: AccessEvaluationLogRepository,
    async_log_sender: Option<mpsc::UnboundedSender<AccessEvaluationLog>>,
    redactor: Option<LogRedactor>,
}

impl AccessEvaluationLogService {
    /// Creates a new access evaluation log service instance
    pub fn new(log_repo: AccessEvaluationLogRepository) -> Self {
        Self {
            log_repo,
            async_log_sender: None,
            redactor: None,
        }
    }

    /// Creates a new access evaluation log service with async logging enabled
    ///
    /// This spawns a background task that processes audit logs asynchronously,
    /// ensuring that logging failures don't block the main access control flow.
    pub fn new_with_async_logging(log_repo: AccessEvaluationLogRepository) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<AccessEvaluationLog>();
        let repo_clone = log_repo.clone();

        // Spawn background task to process audit logs
        tokio::spawn(async move {
            info!("Async audit logging task started");

            while let Some(log) = receiver.recv().await {
                match repo_clone.create(&log).await {
                    Ok(_) => {
                        // Successfully logged, no action needed
                    }
                    Err(e) => {
                        error!(
                            request_id = %log.request_id,
                            user_id = ?log.user_id,
                            error = %e,
                            "Failed to write audit log asynchronously"
                        );
                        // Log failure but don't crash the background task
                    }
                }
            }

            warn!("Async audit logging task terminated");
        });

        Self {
            log_repo,
            async_log_sender: Some(sender),
            redactor: None,
        }
    }

    /// Set a log redactor for sensitive information redaction
    pub fn with_redactor(mut self, redactor: LogRedactor) -> Self {
        self.redactor = Some(redactor);
        self
    }

    /// Apply redaction to context data if redactor is configured
    fn redact_context(&self, context: Option<Value>) -> Option<Value> {
        if let Some(redactor) = &self.redactor {
            context.map(|mut ctx| {
                redactor.redact_json(&mut ctx);
                ctx
            })
        } else {
            context
        }
    }

    /// Logs an access evaluation result with performance metrics
    ///
    /// If async logging is enabled, this will queue the log for background processing
    /// and return immediately without blocking. Otherwise, it will write synchronously.
    ///
    /// Sensitive information in the context will be automatically redacted if a redactor is configured.
    pub async fn log_access_evaluation(
        &self,
        user_id: Option<i64>,
        resource_id: Option<String>,
        resource_type: Option<String>,
        action: String,
        decision: AccessDecision,
        rbac_result: Option<AccessDecision>,
        abac_result: Option<AccessDecision>,
        applied_policies: Option<Vec<String>>,
        evaluation_start: Instant,
        context: Option<Value>,
    ) -> Result<AccessEvaluationLog, AppError> {
        let evaluation_time_ms = evaluation_start.elapsed().as_millis() as i64;
        let request_id = Uuid::new_v4().to_string();

        // Log performance warning if evaluation took too long
        if evaluation_time_ms > 100 {
            warn!(
                request_id = %request_id,
                evaluation_time_ms = evaluation_time_ms,
                user_id = ?user_id,
                resource_type = ?resource_type,
                action = %action,
                "Slow access evaluation detected"
            );
        }

        // Apply redaction to context data
        let redacted_context = self.redact_context(context);

        let log = AccessEvaluationLog::new(
            request_id.clone(),
            user_id,
            resource_id,
            resource_type,
            action,
            decision,
            rbac_result,
            abac_result,
            applied_policies,
            Some(evaluation_time_ms),
            redacted_context,
        );

        // If async logging is enabled, send to background task
        if let Some(sender) = &self.async_log_sender {
            match sender.send(log.clone()) {
                Ok(_) => {
                    // Successfully queued for async processing
                    // Return the log immediately without waiting for DB write
                    Ok(log)
                }
                Err(e) => {
                    error!(
                        request_id = %request_id,
                        error = %e,
                        "Failed to queue audit log for async processing, falling back to sync"
                    );
                    // Fallback to synchronous logging
                    self.log_repo.create(&log).await
                }
            }
        } else {
            // Synchronous logging (legacy behavior)
            self.log_repo.create(&log).await
        }
    }

    /// Logs an access evaluation result with a specific request ID
    ///
    /// This variant allows passing a pre-generated request ID for correlation.
    /// Sensitive information in the context will be automatically redacted if a redactor is configured.
    pub async fn log_access_evaluation_with_request_id(
        &self,
        request_id: String,
        user_id: Option<i64>,
        resource_id: Option<String>,
        resource_type: Option<String>,
        action: String,
        decision: AccessDecision,
        rbac_result: Option<AccessDecision>,
        abac_result: Option<AccessDecision>,
        applied_policies: Option<Vec<String>>,
        evaluation_time_ms: Option<i64>,
        context: Option<Value>,
    ) -> Result<AccessEvaluationLog, AppError> {
        // Apply redaction to context data
        let redacted_context = self.redact_context(context);

        let log = AccessEvaluationLog::new(
            request_id.clone(),
            user_id,
            resource_id,
            resource_type,
            action,
            decision,
            rbac_result,
            abac_result,
            applied_policies,
            evaluation_time_ms,
            redacted_context,
        );

        // If async logging is enabled, send to background task
        if let Some(sender) = &self.async_log_sender {
            match sender.send(log.clone()) {
                Ok(_) => {
                    // Successfully queued for async processing
                    Ok(log)
                }
                Err(e) => {
                    error!(
                        request_id = %request_id,
                        error = %e,
                        "Failed to queue audit log for async processing, falling back to sync"
                    );
                    // Fallback to synchronous logging
                    self.log_repo.create(&log).await
                }
            }
        } else {
            // Synchronous logging (legacy behavior)
            self.log_repo.create(&log).await
        }
    }

    /// Logs a batch of access evaluations (for performance optimization)
    pub async fn log_batch_access_evaluations(
        &self,
        evaluations: Vec<AccessEvaluationLog>,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        let mut results = Vec::new();

        for evaluation in evaluations {
            let result = self.log_repo.create(&evaluation).await?;
            results.push(result);
        }

        info!("Logged {} access evaluations in batch", results.len());
        Ok(results)
    }

    /// Retrieves access evaluation logs with filtering and pagination
    pub async fn get_access_logs(
        &self,
        filter: AccessEvaluationLogFilter,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        self.log_repo.list(&filter).await
    }

    /// Gets an access evaluation log by its ID
    pub async fn get_log_by_id(&self, id: i64) -> Result<Option<AccessEvaluationLog>, AppError> {
        self.log_repo.find_by_id(id).await
    }

    /// Gets access evaluation logs by request ID
    pub async fn get_logs_by_request_id(
        &self,
        request_id: &str,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        self.log_repo.find_by_request_id(request_id).await
    }

    /// Counts access evaluation logs matching the filter
    pub async fn count_logs(&self, filter: &AccessEvaluationLogFilter) -> Result<i64, AppError> {
        self.log_repo.count(filter).await
    }

    /// Gets performance metrics for a time period
    pub async fn get_performance_metrics(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<AccessEvaluationMetrics, AppError> {
        if start_date >= end_date {
            return Err(AppError::BadRequest(
                "Start date must be before end date".to_string(),
            ));
        }

        self.log_repo.get_metrics(start_date, end_date).await
    }

    /// Gets daily performance metrics for the last N days
    pub async fn get_daily_metrics(
        &self,
        days: i64,
    ) -> Result<Vec<AccessEvaluationMetrics>, AppError> {
        if days <= 0 || days > 365 {
            return Err(AppError::BadRequest(
                "Days must be between 1 and 365".to_string(),
            ));
        }

        let mut metrics = Vec::new();
        let end_date = Utc::now();

        for i in 0..days {
            let day_end = end_date - Duration::days(i);
            let day_start = day_end - Duration::days(1);

            let daily_metrics = self.log_repo.get_metrics(day_start, day_end).await?;
            metrics.push(daily_metrics);
        }

        Ok(metrics)
    }

    /// Gets recent access logs for a specific user
    pub async fn get_recent_user_access(
        &self,
        user_id: i64,
        limit: i64,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        if limit <= 0 || limit > 1000 {
            return Err(AppError::BadRequest(
                "Limit must be between 1 and 1000".to_string(),
            ));
        }

        self.log_repo.get_recent_by_user(user_id, limit).await
    }

    /// Gets access patterns for security analysis
    pub async fn get_access_patterns(
        &self,
        user_id: Option<i64>,
        resource_type: Option<String>,
        hours: i64,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        let start_date = Utc::now() - Duration::hours(hours);
        let end_date = Utc::now();

        let filter = AccessEvaluationLogFilter {
            user_id,
            resource_type,
            start_date: Some(start_date),
            end_date: Some(end_date),
            limit: Some(1000),
            offset: Some(0),
            ..Default::default()
        };

        self.log_repo.list(&filter).await
    }

    /// Gets denied access attempts for security monitoring
    pub async fn get_denied_access_attempts(
        &self,
        hours: i64,
        limit: Option<i64>,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        let start_date = Utc::now() - Duration::hours(hours);
        let end_date = Utc::now();

        let filter = AccessEvaluationLogFilter {
            decision: Some(AccessDecision::Deny),
            start_date: Some(start_date),
            end_date: Some(end_date),
            limit: limit.or(Some(100)),
            offset: Some(0),
            ..Default::default()
        };

        self.log_repo.list(&filter).await
    }

    /// Cleans up old access evaluation logs
    pub async fn cleanup_old_logs(&self, days: i64) -> Result<u64, AppError> {
        if days <= 0 {
            return Err(AppError::BadRequest(
                "Days must be greater than 0".to_string(),
            ));
        }

        info!(
            "Cleaning up access evaluation logs older than {} days",
            days
        );
        let deleted = self.log_repo.delete_older_than_days(days).await?;

        if deleted > 0 {
            info!("Deleted {} old access evaluation log entries", deleted);
        }

        Ok(deleted)
    }

    /// Generates a security report for a time period
    pub async fn generate_security_report(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<SecurityReport, AppError> {
        let metrics = self.get_performance_metrics(start_date, end_date).await?;

        let denied_attempts = self
            .get_denied_access_attempts((end_date - start_date).num_hours(), Some(50))
            .await?;

        let suspicious_patterns = self.detect_suspicious_patterns(&denied_attempts);

        Ok(SecurityReport {
            period_start: start_date,
            period_end: end_date,
            metrics,
            denied_attempts,
            suspicious_patterns,
        })
    }

    /// Detects suspicious access patterns
    fn detect_suspicious_patterns(
        &self,
        denied_attempts: &[AccessEvaluationLog],
    ) -> Vec<SuspiciousPattern> {
        let mut patterns = Vec::new();

        // Group by user_id and count denied attempts
        let mut user_denials: std::collections::HashMap<i64, i64> =
            std::collections::HashMap::new();

        for attempt in denied_attempts {
            if let Some(user_id) = attempt.user_id {
                *user_denials.entry(user_id).or_insert(0) += 1;
            }
        }

        // Flag users with excessive denied attempts
        for (user_id, count) in user_denials {
            if count > 10 {
                patterns.push(SuspiciousPattern {
                    pattern_type: "excessive_denied_attempts".to_string(),
                    user_id: Some(user_id),
                    resource_type: None,
                    count,
                    description: format!("User {} has {} denied access attempts", user_id, count),
                });
            }
        }

        patterns
    }

    /// Get logs with filters for API
    pub async fn get_logs_with_filters(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
        result: Option<&str>,
        evaluation_type: Option<&str>,
        date_range: Option<&str>,
    ) -> Result<Vec<AccessEvaluationLog>, AppError> {
        self.log_repo
            .list_with_filters(limit, offset, search, result, evaluation_type, date_range)
            .await
    }

    /// Count logs with filters
    pub async fn count_logs_with_filters(
        &self,
        search: Option<&str>,
        result: Option<&str>,
        evaluation_type: Option<&str>,
        date_range: Option<&str>,
    ) -> Result<i64, AppError> {
        self.log_repo
            .count_with_filters(search, result, evaluation_type, date_range)
            .await
    }

    /// Get statistics
    pub async fn get_stats(
        &self,
        search: Option<&str>,
        result: Option<&str>,
        evaluation_type: Option<&str>,
        date_range: Option<&str>,
    ) -> Result<StatsResponse, AppError> {
        let total = self.count_logs_with_filters(search, result, evaluation_type, date_range).await?;
        let allowed = self.count_logs_with_filters(search, Some("allowed"), evaluation_type, date_range).await?;
        let denied = self.count_logs_with_filters(search, Some("denied"), evaluation_type, date_range).await?;
        let abac_count = self.count_logs_with_filters(search, result, Some("abac"), date_range).await?;
        let rbac_count = self.count_logs_with_filters(search, result, Some("rbac"), date_range).await?;
        let hybrid_count = self.count_logs_with_filters(search, result, Some("hybrid"), date_range).await?;

        Ok(StatsResponse {
            total,
            allowed,
            denied,
            abac_count,
            rbac_count,
            hybrid_count,
        })
    }
}

/// Security report containing access evaluation metrics and suspicious patterns
#[derive(Debug, Clone)]
pub struct SecurityReport {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub metrics: AccessEvaluationMetrics,
    pub denied_attempts: Vec<AccessEvaluationLog>,
    pub suspicious_patterns: Vec<SuspiciousPattern>,
}

/// Suspicious access pattern detected in the logs
#[derive(Debug, Clone)]
pub struct SuspiciousPattern {
    pub pattern_type: String,
    pub user_id: Option<i64>,
    pub resource_type: Option<String>,
    pub count: i64,
    pub description: String,
}
