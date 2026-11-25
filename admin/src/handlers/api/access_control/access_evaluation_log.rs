use crate::{
    errors::AppError,
    models::dto::access_evaluation_log::{
        AccessEvaluationLogListResponse, AccessEvaluationLogQuery, AccessEvaluationLogResponse,
        AccessEvaluationMetricsResponse, MetricsQuery, ResourceDenialSummary,
        SecurityReportResponse, UserDenialSummary,
    },
    AppState,
};
use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_access_logs))
        .route("/detail", get(get_access_log))
        .route("/metrics", get(get_metrics))
        .route("/metrics/daily", get(get_daily_metrics))
        .route("/security-report", get(get_security_report))
        .route("/denied-attempts", get(get_denied_attempts))
        .route("/by-request", get(get_logs_by_request))
        .route("/user-recent", get(get_user_recent_access))
        .layer(middleware::from_fn(crate::middleware::auth))
}

/// List access evaluation logs with filtering and pagination
async fn list_access_logs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AccessEvaluationLogQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    let page = (offset / limit) + 1;

    let filter = query.clone().into();
    let logs = state
        .service
        .access_evaluation_log_service
        .get_access_logs(filter)
        .await?;

    let total_count = state
        .service
        .access_evaluation_log_service
        .count_logs(&query.into())
        .await?;

    let total_pages = (total_count + limit - 1) / limit;

    let response = AccessEvaluationLogListResponse {
        logs: logs
            .into_iter()
            .map(AccessEvaluationLogResponse::from)
            .collect(),
        total_count,
        page,
        per_page: limit,
        total_pages,
    };

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Query parameters for getting a specific log
#[derive(Debug, Deserialize)]
pub struct LogDetailQuery {
    pub id: i64,
}

/// Get a specific access evaluation log by ID
async fn get_access_log(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LogDetailQuery>,
) -> Result<impl IntoResponse, AppError> {
    match state
        .service
        .access_evaluation_log_service
        .get_log_by_id(query.id)
        .await?
    {
        Some(log) => {
            let response = AccessEvaluationLogResponse::from(log);
            Ok((StatusCode::OK, Json(response)).into_response())
        }
        None => Err(AppError::NotFound(
            format!("Access evaluation log with id {} not found", query.id),
        )),
    }
}

/// Query parameters for getting logs by request ID
#[derive(Debug, Deserialize)]
pub struct RequestIdQuery {
    pub request_id: String,
}

/// Get access evaluation logs by request ID
async fn get_logs_by_request(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RequestIdQuery>,
) -> Result<impl IntoResponse, AppError> {
    let logs = state
        .service
        .access_evaluation_log_service
        .get_logs_by_request_id(&query.request_id)
        .await?;

    let response: Vec<AccessEvaluationLogResponse> = logs
        .into_iter()
        .map(AccessEvaluationLogResponse::from)
        .collect();

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Get performance metrics for a time period
async fn get_metrics(
    State(state): State<Arc<AppState>>,
    Query(query): Query<MetricsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let metrics = state
        .service
        .access_evaluation_log_service
        .get_performance_metrics(query.start_date, query.end_date)
        .await?;

    let response = AccessEvaluationMetricsResponse::from(metrics);
    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Get daily metrics for the last N days
async fn get_daily_metrics(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DailyMetricsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let days = params.days.unwrap_or(7);

    if days <= 0 || days > 365 {
        return Err(AppError::BadRequest(
            "Days must be between 1 and 365".to_string(),
        ));
    }

    let metrics = state
        .service
        .access_evaluation_log_service
        .get_daily_metrics(days)
        .await?;

    let response: Vec<AccessEvaluationMetricsResponse> = metrics
        .into_iter()
        .map(AccessEvaluationMetricsResponse::from)
        .collect();

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Get security report for a time period
async fn get_security_report(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SecurityReportQuery>,
) -> Result<impl IntoResponse, AppError> {
    let end_date = query.end_date.unwrap_or_else(Utc::now);
    let start_date = query
        .start_date
        .unwrap_or_else(|| end_date - Duration::days(7));

    let report = state
        .service
        .access_evaluation_log_service
        .generate_security_report(start_date, end_date)
        .await?;

    // Convert to response DTO
    let recommendations = generate_security_recommendations(&report);
    let metrics = AccessEvaluationMetricsResponse::from(report.metrics);
    let denied_attempts_count = report.denied_attempts.len() as i64;
    let suspicious_patterns_count = report.suspicious_patterns.len() as i64;
    let top_denied_users = extract_user_denial_summaries(&report.denied_attempts);
    let top_denied_resources = extract_resource_denial_summaries(&report.denied_attempts);

    let response = SecurityReportResponse {
        period_start: report.period_start,
        period_end: report.period_end,
        metrics,
        denied_attempts_count,
        suspicious_patterns_count,
        top_denied_users,
        top_denied_resources,
        recommendations,
    };

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Get denied access attempts for security monitoring
async fn get_denied_attempts(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DeniedAttemptsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let hours = params.hours.unwrap_or(24);
    let limit = params.limit;

    let denied_attempts = state
        .service
        .access_evaluation_log_service
        .get_denied_access_attempts(hours, limit)
        .await?;

    let response: Vec<AccessEvaluationLogResponse> = denied_attempts
        .into_iter()
        .map(AccessEvaluationLogResponse::from)
        .collect();

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Get recent access logs for a specific user
async fn get_user_recent_access(
    State(state): State<Arc<AppState>>,
    Query(params): Query<UserRecentAccessQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = params.user_id;
    let limit = params.limit.unwrap_or(50);

    if limit <= 0 || limit > 1000 {
        return Err(AppError::BadRequest(
            "Limit must be between 1 and 1000".to_string(),
        ));
    }

    let logs = state
        .service
        .access_evaluation_log_service
        .get_recent_user_access(user_id, limit)
        .await?;

    let response: Vec<AccessEvaluationLogResponse> = logs
        .into_iter()
        .map(AccessEvaluationLogResponse::from)
        .collect();

    Ok((StatusCode::OK, Json(response)).into_response())
}

// Helper functions for processing security report data

// Type alias for user statistics: (count, resource_types_map, actions_map)
type UserStats = (i64, HashMap<String, i64>, HashMap<String, i64>);

fn extract_user_denial_summaries(
    denied_attempts: &[crate::models::entities::access_evaluation_log::AccessEvaluationLog],
) -> Vec<UserDenialSummary> {
    use std::collections::HashMap;

    let mut user_stats: HashMap<i64, UserStats> = HashMap::new();

    for attempt in denied_attempts {
        if let Some(user_id) = attempt.user_id {
            let (count, resource_types, actions) =
                user_stats
                    .entry(user_id)
                    .or_insert((0, HashMap::new(), HashMap::new()));
            *count += 1;

            if let Some(resource_type) = &attempt.resource_type {
                *resource_types.entry(resource_type.clone()).or_insert(0) += 1;
            }
            *actions.entry(attempt.action.clone()).or_insert(0) += 1;
        }
    }

    let mut summaries: Vec<UserDenialSummary> = user_stats
        .into_iter()
        .map(|(user_id, (count, resource_types, actions))| {
            let most_common_resource_type = resource_types
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(resource_type, _)| resource_type);

            let most_common_action = actions
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(action, _)| action);

            UserDenialSummary {
                user_id,
                username: None, // Username lookup requires user table join
                denied_count: count,
                most_common_resource_type,
                most_common_action,
            }
        })
        .collect();

    summaries.sort_by(|a, b| b.denied_count.cmp(&a.denied_count));
    summaries.truncate(10); // Top 10 users
    summaries
}

// Type alias for resource statistics: (count, user_set, actions_map)
type ResourceStats = (i64, HashSet<i64>, HashMap<String, i64>);

fn extract_resource_denial_summaries(
    denied_attempts: &[crate::models::entities::access_evaluation_log::AccessEvaluationLog],
) -> Vec<ResourceDenialSummary> {
    use std::collections::{HashMap, HashSet};

    let mut resource_stats: HashMap<String, ResourceStats> = HashMap::new();

    for attempt in denied_attempts {
        if let Some(resource_type) = &attempt.resource_type {
            let (count, users, actions) = resource_stats.entry(resource_type.clone()).or_insert((
                0,
                HashSet::new(),
                HashMap::new(),
            ));
            *count += 1;

            if let Some(user_id) = attempt.user_id {
                users.insert(user_id);
            }
            *actions.entry(attempt.action.clone()).or_insert(0) += 1;
        }
    }

    let mut summaries: Vec<ResourceDenialSummary> = resource_stats
        .into_iter()
        .map(|(resource_type, (count, users, actions))| {
            let most_common_action = actions
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(action, _)| action);

            ResourceDenialSummary {
                resource_type,
                denied_count: count,
                unique_users: users.len() as i64,
                most_common_action,
            }
        })
        .collect();

    summaries.sort_by(|a, b| b.denied_count.cmp(&a.denied_count));
    summaries.truncate(10); // Top 10 resource types
    summaries
}

fn generate_security_recommendations(
    report: &crate::services::access_evaluation_log::SecurityReport,
) -> Vec<String> {
    let mut recommendations = Vec::new();

    // High denial rate recommendation
    if report.metrics.total_evaluations > 0 {
        let denial_rate =
            (report.metrics.deny_count as f64 / report.metrics.total_evaluations as f64) * 100.0;
        if denial_rate > 20.0 {
            recommendations.push(format!(
                "High denial rate detected ({:.1}%). Consider reviewing access policies.",
                denial_rate
            ));
        }
    }

    // Slow evaluation time recommendation
    if report.metrics.average_evaluation_time_ms > 50.0 {
        recommendations.push(format!(
            "Average evaluation time is {:.1}ms. Consider optimizing access control policies.",
            report.metrics.average_evaluation_time_ms
        ));
    }

    // Suspicious patterns recommendation
    if !report.suspicious_patterns.is_empty() {
        recommendations.push(format!(
            "{} suspicious patterns detected. Review user access patterns.",
            report.suspicious_patterns.len()
        ));
    }

    // Default recommendation if no issues
    if recommendations.is_empty() {
        recommendations.push("Access control system is operating normally.".to_string());
    }

    recommendations
}

// Query parameter structs

#[derive(serde::Deserialize)]
struct DailyMetricsQuery {
    days: Option<i64>,
}

#[derive(serde::Deserialize)]
struct SecurityReportQuery {
    start_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
}

#[derive(serde::Deserialize)]
struct DeniedAttemptsQuery {
    hours: Option<i64>,
    limit: Option<i64>,
}

#[derive(serde::Deserialize)]
struct UserRecentAccessQuery {
    user_id: i64,
    limit: Option<i64>,
}
