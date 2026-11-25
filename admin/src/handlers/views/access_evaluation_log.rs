use crate::{
    errors::AppError,
    middleware::UserId,
    models::{
        dto::{
            access_evaluation_log::{
                AccessEvaluationLogQuery, AccessEvaluationLogResponse,
                AccessEvaluationMetricsResponse,
            },
            user::UserResponse,
        },
        entities::access_evaluation_log::AccessDecision,
    },
    utils::render_template,
    AppState,
};
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, Router},
    Extension,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info};

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(access_logs_page))
        .route("/metrics", get(metrics_page))
        .route("/security-report", get(security_report_page))
}

#[derive(Debug, Deserialize)]
pub struct AccessLogQueryParams {
    page: Option<i64>,
    per_page: Option<i64>,
    user_id: Option<i64>,
    resource_type: Option<String>,
    action: Option<String>,
    decision: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MetricsQueryParams {
    days: Option<i64>,
    start_date: Option<String>,
    end_date: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
struct AccessLogsTemplateContext {
    title: &'static str,
    active_page: &'static str,
    user_id: i64,
    current_user: Option<UserResponse>,
    access_logs: Vec<AccessEvaluationLogResponse>,
    total: i64,
    page: i64,
    per_page: i64,
    total_pages: i64,
    start_item: i64,
    end_item: i64,
    filters: AccessLogFilters,
}

#[derive(Serialize, Debug, Clone)]
struct AccessLogFilters {
    user_id: Option<i64>,
    resource_type: Option<String>,
    action: Option<String>,
    decision: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
struct MetricsTemplateContext {
    title: &'static str,
    active_page: &'static str,
    user_id: i64,
    current_user: Option<UserResponse>,
    metrics: Option<AccessEvaluationMetricsResponse>,
    daily_metrics: Vec<AccessEvaluationMetricsResponse>,
    period_days: i64,
}

#[derive(Serialize, Debug, Clone)]
struct SecurityReportTemplateContext {
    title: &'static str,
    active_page: &'static str,
    user_id: i64,
    current_user: Option<UserResponse>,
    report_period_days: i64,
    metrics: Option<AccessEvaluationMetricsResponse>,
    denied_attempts_count: i64,
    suspicious_patterns_count: i64,
    recommendations: Vec<String>,
    recent_denied_attempts: Vec<AccessEvaluationLogResponse>,
}

/// Access evaluation logs page
async fn access_logs_page(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
    Query(params): Query<AccessLogQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    info!(
        "Starting access logs page handler for user_id: {}",
        user_id.0
    );
    debug!("Query params: {:?}", params);

    // Get current user data
    let current_user = match state.service.user_service.get_user_by_id(user_id.0).await {
        Ok(user) => Some(user),
        Err(e) => {
            error!("Error fetching user: {}", e);
            None
        }
    };

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(50).clamp(1, 100);
    let offset = (page - 1) * per_page;

    // Parse dates
    let start_date = params
        .start_date
        .as_ref()
        .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let end_date = params
        .end_date
        .as_ref()
        .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
        .map(|dt| dt.with_timezone(&Utc));

    // Parse decision
    let decision = params
        .decision
        .as_ref()
        .and_then(|d| d.parse::<AccessDecision>().ok());

    let query = AccessEvaluationLogQuery {
        user_id: params.user_id,
        resource_type: params.resource_type.clone(),
        action: params.action.clone(),
        decision,
        start_date,
        end_date,
        limit: Some(per_page),
        offset: Some(offset),
    };

    debug!("Fetching access logs with query: {:?}", query);

    // Get access logs
    let filter = query.clone().into();
    let access_logs = match state
        .service
        .access_evaluation_log_service
        .get_access_logs(filter)
        .await
    {
        Ok(logs) => logs
            .into_iter()
            .map(AccessEvaluationLogResponse::from)
            .collect(),
        Err(e) => {
            error!("Error fetching access logs: {}", e);
            return Err(AppError::InternalServerError(
                "Failed to fetch access logs".to_string(),
            ));
        }
    };

    // Get total count
    let total = match state
        .service
        .access_evaluation_log_service
        .count_logs(&query.into())
        .await
    {
        Ok(total) => total,
        Err(e) => {
            error!("Error counting access logs: {}", e);
            return Err(AppError::InternalServerError(
                "Failed to count access logs".to_string(),
            ));
        }
    };

    let total_pages = (total as f64 / per_page as f64).ceil() as i64;
    let start_item = ((page - 1) * per_page + 1).min(total.max(1));
    let end_item = (page * per_page).min(total);

    let filters = AccessLogFilters {
        user_id: params.user_id,
        resource_type: params.resource_type,
        action: params.action,
        decision: params.decision,
        start_date: params.start_date,
        end_date: params.end_date,
    };

    let context = AccessLogsTemplateContext {
        title: "Access Evaluation Logs",
        active_page: "access_logs",
        user_id: user_id.0,
        current_user,
        access_logs,
        total,
        page,
        per_page,
        total_pages: total_pages.max(1),
        start_item,
        end_item,
        filters,
    };

    render_template(&state.tera, "pages/access_evaluation_logs/list.html", context).await
}

/// Metrics page
async fn metrics_page(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
    Query(params): Query<MetricsQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    info!("Starting metrics page handler for user_id: {}", user_id.0);

    // Get current user data
    let current_user = match state.service.user_service.get_user_by_id(user_id.0).await {
        Ok(user) => Some(user),
        Err(e) => {
            error!("Error fetching user: {}", e);
            None
        }
    };

    let days = params.days.unwrap_or(7).clamp(1, 365);

    // Get overall metrics for the period
    let end_date = Utc::now();
    let start_date = end_date - Duration::days(days);

    let metrics = match state
        .service
        .access_evaluation_log_service
        .get_performance_metrics(start_date, end_date)
        .await
    {
        Ok(metrics) => Some(AccessEvaluationMetricsResponse::from(metrics)),
        Err(e) => {
            error!("Error fetching metrics: {}", e);
            None
        }
    };

    // Get daily metrics
    let daily_metrics = match state
        .service
        .access_evaluation_log_service
        .get_daily_metrics(days)
        .await
    {
        Ok(metrics) => metrics
            .into_iter()
            .map(AccessEvaluationMetricsResponse::from)
            .collect(),
        Err(e) => {
            error!("Error fetching daily metrics: {}", e);
            Vec::new()
        }
    };

    let context = MetricsTemplateContext {
        title: "Access Control Metrics",
        active_page: "access_metrics",
        user_id: user_id.0,
        current_user,
        metrics,
        daily_metrics,
        period_days: days,
    };

    render_template(&state.tera, "pages/access_evaluation_logs/metrics.html", context).await
}

/// Security report page
async fn security_report_page(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
    Query(params): Query<MetricsQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    info!(
        "Starting security report page handler for user_id: {}",
        user_id.0
    );

    // Get current user data
    let current_user = match state.service.user_service.get_user_by_id(user_id.0).await {
        Ok(user) => Some(user),
        Err(e) => {
            error!("Error fetching user: {}", e);
            None
        }
    };

    let days = params.days.unwrap_or(7).clamp(1, 365);
    let end_date = Utc::now();
    let start_date = end_date - Duration::days(days);

    // Generate security report
    let report = match state
        .service
        .access_evaluation_log_service
        .generate_security_report(start_date, end_date)
        .await
    {
        Ok(report) => report,
        Err(e) => {
            error!("Error generating security report: {}", e);
            return Err(AppError::InternalServerError(
                "Failed to generate security report".to_string(),
            ));
        }
    };

    // Get recent denied attempts for display
    let recent_denied_attempts = match state
        .service
        .access_evaluation_log_service
        .get_denied_access_attempts(24, Some(10))
        .await
    {
        Ok(attempts) => attempts
            .into_iter()
            .map(AccessEvaluationLogResponse::from)
            .collect(),
        Err(e) => {
            error!("Error fetching denied attempts: {}", e);
            Vec::new()
        }
    };

    let recommendations = generate_recommendations(&report);
    let metrics = Some(AccessEvaluationMetricsResponse::from(report.metrics));
    let denied_attempts_count = report.denied_attempts.len() as i64;
    let suspicious_patterns_count = report.suspicious_patterns.len() as i64;

    let context = SecurityReportTemplateContext {
        title: "Security Report",
        active_page: "security_report",
        user_id: user_id.0,
        current_user,
        report_period_days: days,
        metrics,
        denied_attempts_count,
        suspicious_patterns_count,
        recommendations,
        recent_denied_attempts,
    };

    render_template(&state.tera, "pages/access_logs/security_report.html", context).await
}

fn generate_recommendations(
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
