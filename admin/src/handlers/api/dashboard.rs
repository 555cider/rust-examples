use crate::config::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::error;

pub fn route() -> Router<Arc<AppState>> {
    Router::new().route("/", get(dashboard_data))
}

#[derive(Deserialize)]
struct DashboardQuery {
    #[serde(default = "default_range")]
    range: String,
}

fn default_range() -> String {
    "day".to_string()
}

#[derive(Serialize)]
struct DashboardResponse {
    dashboard_data: DashboardData,
}

#[derive(Serialize)]
struct DashboardData {
    total_users: i64,
    active_users: i64,
    active_percent: f64,
    inactive_users: i64,
    new_users_today: i64,
    new_users_week: i64,
    total_roles: i64,
    total_permissions: i64,
    total_logs: i64,
    daily_active_users: Vec<DailyActive>,
    recent_history: Vec<HistoryItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_evaluation_log_stats: Option<AccessEvaluationLogStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recent_denied_attempts: Option<Vec<DeniedAttempt>>,
}

#[derive(Serialize)]
struct AccessEvaluationLogStats {
    total_evaluations: i64,
    avg_evaluation_time_ms: f64,
    allowed_count: i64,
    allowed_percent: f64,
    denied_count: i64,
    denied_percent: f64,
    rbac_count: i64,
    rbac_percent: f64,
    abac_count: i64,
    abac_percent: f64,
    hybrid_count: i64,
    hybrid_percent: f64,
}

#[derive(Serialize)]
struct DeniedAttempt {
    user_id: Option<i64>,
    resource_type: Option<String>,
    action: String,
    created_at: String,
}

#[derive(Serialize)]
struct DailyActive {
    date: String,
    count: i64,
}

#[derive(Serialize)]
struct HistoryItem {
    username: String,
    action: String,
    ip_address: Option<String>,
    created_at: String,
}

async fn dashboard_data(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DashboardQuery>,
) -> impl IntoResponse {
    match fetch_dashboard_data(&state, &query.range).await {
        Ok(data) => Json(DashboardResponse {
            dashboard_data: data,
        })
        .into_response(),
        Err(e) => {
            error!("Failed to fetch dashboard data: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to fetch dashboard data",
                    "message": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

async fn fetch_dashboard_data(state: &AppState, range: &str) -> anyhow::Result<DashboardData> {
    let pool = &state.pool;

    // Calculate date range
    let (start_date, days) = match range {
        "week" => (Utc::now() - Duration::days(7), 7),
        "month" => (Utc::now() - Duration::days(30), 30),
        _ => (Utc::now() - Duration::hours(24), 1), // "day" or default
    };

    // Total users (Always total)
    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_user")
        .fetch_one(pool)
        .await?;

    // Active users (logged in within range)
    let active_users: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM admin_user WHERE last_login_at >= ?")
            .bind(start_date.naive_utc())
            .fetch_one(pool)
            .await?;

    let inactive_users = total_users - active_users;
    let active_percent = if total_users > 0 {
        (active_users as f64 / total_users as f64) * 100.0
    } else {
        0.0
    };

    // New users in range
    let new_users_today: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM admin_user WHERE created_at >= ?")
            .bind(start_date.naive_utc())
            .fetch_one(pool)
            .await?;

    // New users comparison (previous period)
    let previous_start_date = start_date - (start_date - (Utc::now() - Duration::days(days)));
    let new_users_week: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM admin_user WHERE created_at >= ? AND created_at < ?")
            .bind(previous_start_date.naive_utc())
            .bind(start_date.naive_utc())
            .fetch_one(pool)
            .await?;

    // Total roles (Always total)
    let total_roles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roles")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    // Total permissions (Always total)
    let total_permissions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM permission")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    // Total logs (in range)
    let total_logs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history WHERE created_at >= ?")
        .bind(start_date.naive_utc())
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    // Daily active users based on range
    let mut daily_active_users = Vec::new();
    for i in (0..days).rev() {
        let date = Utc::now() - Duration::days(i);
        let date_str = date.format("%Y-%m-%d").to_string();
        let date_start = date.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let date_end = date.date_naive().and_hms_opt(23, 59, 59).unwrap();

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT user_id) FROM history WHERE created_at >= ? AND created_at <= ?",
        )
        .bind(date_start)
        .bind(date_end)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        daily_active_users.push(DailyActive {
            date: date_str,
            count,
        });
    }

    // Recent history (last 20 items for API)
    let recent_history: Vec<HistoryItem> =
        sqlx::query_as::<_, (String, String, Option<String>, String)>(
            "SELECT
            COALESCE(au.username, 'System') as username,
            h.action,
            h.ip_address,
            datetime(h.created_at) as created_at
         FROM history h
         LEFT JOIN admin_user au ON h.user_id = au.id
         ORDER BY h.created_at DESC
         LIMIT 20",
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(username, action, ip_address, created_at)| HistoryItem {
            username,
            action,
            ip_address,
            created_at,
        })
        .collect();

    // Access log statistics based on range
    let access_evaluation_log_stats = fetch_access_evaluation_log_stats(pool, start_date).await.ok();
    let recent_denied_attempts = fetch_recent_denied_attempts(pool, 10).await.ok();

    Ok(DashboardData {
        total_users,
        active_users,
        active_percent,
        inactive_users,
        new_users_today,
        new_users_week,
        total_roles,
        total_permissions,
        total_logs,
        daily_active_users,
        recent_history,
        access_evaluation_log_stats,
        recent_denied_attempts,
    })
}

async fn fetch_access_evaluation_log_stats(
    pool: &sqlx::SqlitePool,
    since: chrono::DateTime<Utc>,
) -> anyhow::Result<AccessEvaluationLogStats> {
    // Total evaluations
    let total_evaluations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM access_evaluation_log WHERE created_at >= ?")
            .bind(since.naive_utc())
            .fetch_one(pool)
            .await?;

    if total_evaluations == 0 {
        return Ok(AccessEvaluationLogStats {
            total_evaluations: 0,
            avg_evaluation_time_ms: 0.0,
            allowed_count: 0,
            allowed_percent: 0.0,
            denied_count: 0,
            denied_percent: 0.0,
            rbac_count: 0,
            rbac_percent: 0.0,
            abac_count: 0,
            abac_percent: 0.0,
            hybrid_count: 0,
            hybrid_percent: 0.0,
        });
    }

    // Average evaluation time
    let avg_evaluation_time_ms: f64 = sqlx::query_scalar(
        "SELECT COALESCE(AVG(evaluation_time_ms), 0.0) FROM access_evaluation_log WHERE created_at >= ?",
    )
    .bind(since.naive_utc())
    .fetch_one(pool)
    .await?;

    // Allowed and denied counts
    let allowed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM access_evaluation_log WHERE created_at >= ? AND decision = 'PERMIT'",
    )
    .bind(since.naive_utc())
    .fetch_one(pool)
    .await?;

    let denied_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM access_evaluation_log WHERE created_at >= ? AND decision = 'DENY'",
    )
    .bind(since.naive_utc())
    .fetch_one(pool)
    .await?;

    let allowed_percent = (allowed_count as f64 / total_evaluations as f64) * 100.0;
    let denied_percent = (denied_count as f64 / total_evaluations as f64) * 100.0;

    // RBAC-only evaluations (rbac_result is PERMIT, abac_result is NULL or NOT_APPLICABLE)
    let rbac_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM access_evaluation_log
         WHERE created_at >= ?
         AND rbac_result = 'PERMIT'
         AND (abac_result IS NULL OR abac_result = 'NOT_APPLICABLE')",
    )
    .bind(since.naive_utc())
    .fetch_one(pool)
    .await?;

    // ABAC-only evaluations (abac_result is PERMIT, rbac_result is NULL or NOT_APPLICABLE)
    let abac_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM access_evaluation_log
         WHERE created_at >= ?
         AND abac_result = 'PERMIT'
         AND (rbac_result IS NULL OR rbac_result = 'NOT_APPLICABLE')",
    )
    .bind(since.naive_utc())
    .fetch_one(pool)
    .await?;

    // Hybrid evaluations (both rbac_result and abac_result are PERMIT)
    let hybrid_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM access_evaluation_log
         WHERE created_at >= ?
         AND rbac_result = 'PERMIT'
         AND abac_result = 'PERMIT'",
    )
    .bind(since.naive_utc())
    .fetch_one(pool)
    .await?;

    let rbac_percent = (rbac_count as f64 / total_evaluations as f64) * 100.0;
    let abac_percent = (abac_count as f64 / total_evaluations as f64) * 100.0;
    let hybrid_percent = (hybrid_count as f64 / total_evaluations as f64) * 100.0;

    Ok(AccessEvaluationLogStats {
        total_evaluations,
        avg_evaluation_time_ms,
        allowed_count,
        allowed_percent,
        denied_count,
        denied_percent,
        rbac_count,
        rbac_percent,
        abac_count,
        abac_percent,
        hybrid_count,
        hybrid_percent,
    })
}

async fn fetch_recent_denied_attempts(
    pool: &sqlx::SqlitePool,
    limit: i64,
) -> anyhow::Result<Vec<DeniedAttempt>> {
    let denied_attempts = sqlx::query_as::<_, (Option<i64>, Option<String>, String, String)>(
        "SELECT user_id, resource_type, action, datetime(created_at) as created_at
         FROM access_evaluation_log
         WHERE decision = 'DENY'
         ORDER BY created_at DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(user_id, resource_type, action, created_at)| DeniedAttempt {
        user_id,
        resource_type,
        action,
        created_at,
    })
    .collect();

    Ok(denied_attempts)
}
