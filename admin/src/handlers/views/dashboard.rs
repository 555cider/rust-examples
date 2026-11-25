use crate::{middleware::UserId, AppState};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::{error::Error, sync::Arc};
use tera::Context;
use tracing::error;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(dashboard_page))
        .route("/dashboard", get(dashboard_page))
}

async fn dashboard_page(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
) -> impl IntoResponse {
    let mut context = Context::new();
    context.insert("title", "대시보드");
    context.insert("active_page", "dashboard");
    context.insert("user_id", &user_id.0);

    // Add current user info for the template
    match state.service.user_service.get_user_by_id(user_id.0).await {
        Ok(current_user) => {
            context.insert("current_user", &current_user);
        }
        Err(e) => {
            error!("Failed to get current user: {}", e);
            // Insert minimal user object to avoid template errors
            context.insert(
                "current_user",
                &serde_json::json!({
                    "id": user_id.0,
                    "username": "Unknown",
                    "role_id": 0
                }),
            );
        }
    }

    // Fetch initial dashboard data for server-side rendering
    let dashboard_data = match fetch_dashboard_data(&state).await {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to fetch dashboard data: {}", e);
            // Provide empty data structure for template rendering
            DashboardData {
                total_users: 0,
                active_users: 0,
                active_percent: 0.0,
                inactive_users: 0,
                new_users_today: 0,
                new_users_week: 0,
                total_roles: 0,
                total_permissions: 0,
                total_logs: 0,
                daily_active_users: Vec::new(),
                recent_history: Vec::new(),
                access_evaluation_log_stats: None,
                recent_denied_attempts: Vec::new(),
            }
        }
    };
    context.insert("dashboard_data", &dashboard_data);

    match state.tera.render("pages/dashboard/index.html", &context) {
        Ok(s) => Html(s).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            // Log error details
            if let Some(source) = e.source() {
                error!("Error source: {}", source);
            }
            // Try to log context
            match serde_json::to_string_pretty(&context.into_json()) {
                Ok(ctx_str) => error!("Context: {}", ctx_str),
                Err(_) => error!("Failed to serialize context"),
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Template error: {}", e),
            )
                .into_response()
        }
    }
}

#[derive(serde::Serialize)]
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
    access_evaluation_log_stats: Option<AccessEvaluationLogStats>,
    recent_denied_attempts: Vec<DeniedAttempt>,
}

#[derive(serde::Serialize)]
struct DailyActive {
    date: String,
    count: i64,
}

#[derive(serde::Serialize)]
struct HistoryItem {
    username: String,
    action: String,
    ip_address: Option<String>,
    created_at: String,
}

#[derive(serde::Serialize)]
struct AccessEvaluationLogStats {
    total_evaluations: i64,
    allowed_count: i64,
    denied_count: i64,
    allowed_percent: f64,
    denied_percent: f64,
    rbac_count: i64,
    abac_count: i64,
    hybrid_count: i64,
    rbac_percent: f64,
    abac_percent: f64,
    hybrid_percent: f64,
    avg_evaluation_time_ms: f64,
}

#[derive(serde::Serialize)]
struct DeniedAttempt {
    user_id: Option<i64>,
    resource_type: Option<String>,
    action: String,
    created_at: String,
}

async fn fetch_dashboard_data(state: &AppState) -> anyhow::Result<DashboardData> {
    use chrono::{Duration, Utc};

    let pool = &state.pool;

    // Total users
    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_user")
        .fetch_one(pool)
        .await?;

    // Active users (logged in within last 30 days)
    let thirty_days_ago = Utc::now() - Duration::days(30);
    let active_users: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM admin_user WHERE last_login_at >= ?")
            .bind(thirty_days_ago.naive_utc())
            .fetch_one(pool)
            .await?;

    let inactive_users = total_users - active_users;
    let active_percent = if total_users > 0 {
        (active_users as f64 / total_users as f64) * 100.0
    } else {
        0.0
    };

    // New users today
    let today_start = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    let new_users_today: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM admin_user WHERE created_at >= ?")
            .bind(today_start)
            .fetch_one(pool)
            .await?;

    // New users this week
    let week_ago = Utc::now() - Duration::days(7);
    let new_users_week: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM admin_user WHERE created_at >= ?")
            .bind(week_ago.naive_utc())
            .fetch_one(pool)
            .await?;

    // Total roles
    let total_roles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roles")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    // Total permissions
    let total_permissions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM permission")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    // Total logs (history)
    let total_logs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    // Daily active users (last 7 days)
    let mut daily_active_users = Vec::new();
    for i in (0..7).rev() {
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

    // Recent history (last 10 items)
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
         LIMIT 10",
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

    // Access log statistics (last 24 hours)
    let access_evaluation_log_stats = fetch_access_evaluation_log_stats(pool).await.ok();

    // Recent denied attempts (last 10)
    let recent_denied_attempts = fetch_recent_denied_attempts(pool).await.unwrap_or_default();

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

async fn fetch_access_evaluation_log_stats(pool: &sqlx::SqlitePool) -> anyhow::Result<AccessEvaluationLogStats> {
    use chrono::{Duration, Utc};

    let twenty_four_hours_ago = Utc::now() - Duration::hours(24);

    // Get total evaluations and decision counts
    let stats = sqlx::query_as::<_, (i64, i64, i64, f64)>(
        "SELECT 
            COUNT(*) as total,
            SUM(CASE WHEN decision = 'PERMIT' THEN 1 ELSE 0 END) as allowed,
            SUM(CASE WHEN decision = 'DENY' THEN 1 ELSE 0 END) as denied,
            AVG(COALESCE(evaluation_time_ms, 0)) as avg_time
         FROM access_evaluation_log 
         WHERE created_at >= ?",
    )
    .bind(twenty_four_hours_ago)
    .fetch_one(pool)
    .await?;

    let (total_evaluations, allowed_count, denied_count, avg_evaluation_time_ms) = stats;

    // Get evaluation type counts
    let type_stats = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT 
            SUM(CASE WHEN rbac_result IS NOT NULL AND abac_result IS NULL THEN 1 ELSE 0 END) as rbac_only,
            SUM(CASE WHEN rbac_result IS NULL AND abac_result IS NOT NULL THEN 1 ELSE 0 END) as abac_only,
            SUM(CASE WHEN rbac_result IS NOT NULL AND abac_result IS NOT NULL THEN 1 ELSE 0 END) as hybrid
         FROM access_evaluation_log 
         WHERE created_at >= ?",
    )
    .bind(twenty_four_hours_ago)
    .fetch_one(pool)
    .await?;

    let (rbac_count, abac_count, hybrid_count) = type_stats;

    let allowed_percent = if total_evaluations > 0 {
        (allowed_count as f64 / total_evaluations as f64) * 100.0
    } else {
        0.0
    };

    let denied_percent = if total_evaluations > 0 {
        (denied_count as f64 / total_evaluations as f64) * 100.0
    } else {
        0.0
    };

    let rbac_percent = if total_evaluations > 0 {
        (rbac_count as f64 / total_evaluations as f64) * 100.0
    } else {
        0.0
    };

    let abac_percent = if total_evaluations > 0 {
        (abac_count as f64 / total_evaluations as f64) * 100.0
    } else {
        0.0
    };

    let hybrid_percent = if total_evaluations > 0 {
        (hybrid_count as f64 / total_evaluations as f64) * 100.0
    } else {
        0.0
    };

    Ok(AccessEvaluationLogStats {
        total_evaluations,
        allowed_count,
        denied_count,
        allowed_percent,
        denied_percent,
        rbac_count,
        abac_count,
        hybrid_count,
        rbac_percent,
        abac_percent,
        hybrid_percent,
        avg_evaluation_time_ms,
    })
}

async fn fetch_recent_denied_attempts(pool: &sqlx::SqlitePool) -> anyhow::Result<Vec<DeniedAttempt>> {
    use chrono::{Duration, Utc};

    let twenty_four_hours_ago = Utc::now() - Duration::hours(24);

    let attempts = sqlx::query_as::<_, (Option<i64>, Option<String>, String, String)>(
        "SELECT 
            user_id,
            resource_type,
            action,
            datetime(created_at) as created_at
         FROM access_evaluation_log 
         WHERE decision = 'DENY' AND created_at >= ?
         ORDER BY created_at DESC
         LIMIT 10",
    )
    .bind(twenty_four_hours_ago)
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

    Ok(attempts)
}
