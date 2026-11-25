use crate::models::entities::access_evaluation_log::{
    AccessDecision, AccessEvaluationLog, AccessEvaluationLogFilter, AccessEvaluationMetrics,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Response DTO for access evaluation log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessEvaluationLogResponse {
    pub id: i64,
    pub request_id: String,
    pub user_id: Option<i64>,
    pub username: Option<String>, // Joined from admin_user table
    pub resource_id: Option<String>,
    pub resource_type: Option<String>,
    pub action: String,
    pub decision: AccessDecision,
    pub rbac_result: Option<AccessDecision>,
    pub abac_result: Option<AccessDecision>,
    pub applied_policies: Vec<String>,
    pub evaluation_time_ms: Option<i64>,
    pub context: Option<Value>,
    pub created_at: DateTime<Utc>,
}

impl From<AccessEvaluationLog> for AccessEvaluationLogResponse {
    fn from(log: AccessEvaluationLog) -> Self {
        let applied_policies = log.get_applied_policies();
        let context = log.get_context();

        Self {
            id: log.id,
            request_id: log.request_id,
            user_id: log.user_id,
            username: None, // Will be populated by service layer if needed
            resource_id: log.resource_id,
            resource_type: log.resource_type,
            action: log.action,
            decision: log.decision,
            rbac_result: log.rbac_result,
            abac_result: log.abac_result,
            applied_policies,
            evaluation_time_ms: log.evaluation_time_ms,
            context,
            created_at: log.created_at,
        }
    }
}

/// Request DTO for querying access evaluation logs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccessEvaluationLogQuery {
    pub user_id: Option<i64>,
    pub resource_type: Option<String>,
    pub action: Option<String>,
    pub decision: Option<AccessDecision>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl From<AccessEvaluationLogQuery> for AccessEvaluationLogFilter {
    fn from(query: AccessEvaluationLogQuery) -> Self {
        Self {
            user_id: query.user_id,
            resource_type: query.resource_type,
            action: query.action,
            decision: query.decision,
            start_date: query.start_date,
            end_date: query.end_date,
            limit: query.limit,
            offset: query.offset,
        }
    }
}

/// Response DTO for access evaluation metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessEvaluationMetricsResponse {
    pub total_evaluations: i64,
    pub permit_count: i64,
    pub deny_count: i64,
    pub not_applicable_count: i64,
    pub permit_percentage: f64,
    pub deny_percentage: f64,
    pub average_evaluation_time_ms: f64,
    pub max_evaluation_time_ms: i64,
    pub min_evaluation_time_ms: i64,
    pub rbac_only_count: i64,
    pub abac_only_count: i64,
    pub hybrid_count: i64,
    pub rbac_only_percentage: f64,
    pub abac_only_percentage: f64,
    pub hybrid_percentage: f64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

impl From<AccessEvaluationMetrics> for AccessEvaluationMetricsResponse {
    fn from(metrics: AccessEvaluationMetrics) -> Self {
        let total = metrics.total_evaluations as f64;

        Self {
            total_evaluations: metrics.total_evaluations,
            permit_count: metrics.permit_count,
            deny_count: metrics.deny_count,
            not_applicable_count: metrics.not_applicable_count,
            permit_percentage: if total > 0.0 {
                (metrics.permit_count as f64 / total) * 100.0
            } else {
                0.0
            },
            deny_percentage: if total > 0.0 {
                (metrics.deny_count as f64 / total) * 100.0
            } else {
                0.0
            },
            average_evaluation_time_ms: metrics.average_evaluation_time_ms,
            max_evaluation_time_ms: metrics.max_evaluation_time_ms,
            min_evaluation_time_ms: metrics.min_evaluation_time_ms,
            rbac_only_count: metrics.rbac_only_count,
            abac_only_count: metrics.abac_only_count,
            hybrid_count: metrics.hybrid_count,
            rbac_only_percentage: if total > 0.0 {
                (metrics.rbac_only_count as f64 / total) * 100.0
            } else {
                0.0
            },
            abac_only_percentage: if total > 0.0 {
                (metrics.abac_only_count as f64 / total) * 100.0
            } else {
                0.0
            },
            hybrid_percentage: if total > 0.0 {
                (metrics.hybrid_count as f64 / total) * 100.0
            } else {
                0.0
            },
            period_start: metrics.period_start,
            period_end: metrics.period_end,
        }
    }
}

/// Response DTO for paginated access evaluation logs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessEvaluationLogListResponse {
    pub logs: Vec<AccessEvaluationLogResponse>,
    pub total_count: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

/// Response DTO for security report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReportResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub metrics: AccessEvaluationMetricsResponse,
    pub denied_attempts_count: i64,
    pub suspicious_patterns_count: i64,
    pub top_denied_users: Vec<UserDenialSummary>,
    pub top_denied_resources: Vec<ResourceDenialSummary>,
    pub recommendations: Vec<String>,
}

/// Summary of denied attempts by user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDenialSummary {
    pub user_id: i64,
    pub username: Option<String>,
    pub denied_count: i64,
    pub most_common_resource_type: Option<String>,
    pub most_common_action: Option<String>,
}

/// Summary of denied attempts by resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDenialSummary {
    pub resource_type: String,
    pub denied_count: i64,
    pub unique_users: i64,
    pub most_common_action: Option<String>,
}

/// Request DTO for metrics query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsQuery {
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub granularity: Option<MetricsGranularity>,
}

/// Granularity for metrics aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricsGranularity {
    Hour,
    Day,
    Week,
    Month,
}
