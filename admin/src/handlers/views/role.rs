use crate::{
    errors::AppError,
    middleware::UserId,
    models::dto::{
        common::ListQueryParams, role::{CreateRoleRequest, UpdateRoleRequest, RoleResponse},
        user::UserResponse,
    },
    utils::templates,
    AppState,
};
use axum::{
    extract::{Form, Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Extension, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tera::Context;

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(role_list))
        .route("/create", get(role_create_page).post(create_role))
        .route("/edit/{id}", get(role_edit_page))
        .route("/update/{id}", axum::routing::post(update_role))
        .route("/delete/{id}", axum::routing::post(delete_role))
}

#[derive(Debug, Deserialize)]
pub struct RoleListQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub q: Option<String>,
    pub sort_by: Option<String>,
    pub order: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TemplateContext {
    title: &'static str,
    active_page: &'static str,
    user_id: i64,
    current_user: Option<UserResponse>,
    roles: Option<Vec<RoleResponse>>,
    role: Option<RoleResponse>,
    total: i64,
    page: i64,
    per_page: i64,
    total_pages: i64,
    q: Option<String>,
    sort_by: Option<String>,
    order: Option<String>,
    status: Option<String>,
}

impl From<TemplateContext> for Context {
    fn from(ctx: TemplateContext) -> Self {
        let mut context = Context::new();
        context.insert("title", &ctx.title);
        context.insert("active_page", &ctx.active_page);
        context.insert("user_id", &ctx.user_id);
        context.insert("current_user", &ctx.current_user);
        context.insert("roles", &ctx.roles);
        context.insert("role", &ctx.role);
        context.insert("total", &ctx.total);
        context.insert("page", &ctx.page);
        context.insert("per_page", &ctx.per_page);
        context.insert("total_pages", &ctx.total_pages);
        context.insert("q", &ctx.q);
        context.insert("sort_by", &ctx.sort_by);
        context.insert("order", &ctx.order);
        context.insert("status", &ctx.status);
        context
    }
}

// Using templates::render_template instead of local implementation

pub async fn role_list(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
    Query(query): Query<RoleListQuery>,
) -> Result<Response, AppError> {
    // Get current user info for the template
    let current_user = state
        .service
        .user_service
        .get_user_by_id(user_id.0)
        .await
        .ok();

    // Set up pagination
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(10).clamp(1, 100);
    let _offset = (page - 1) * limit;

    // Prepare query parameters
    let query_params = ListQueryParams {
        page: Some(page),
        limit: Some(limit),
        sort_by: query.sort_by.clone(),
        order: query.order.clone(),
        q: query.q.clone(),
        status: query.status.clone(),
    };

    let roles = state
        .service
        .role_service
        .get_roles(query_params)
        .await?;

    // For now, we'll set total to the length of roles
    // In a real app, you might want to get the total count separately
    let total = roles.len() as i64;

    let total_pages = (total as f64 / limit as f64).ceil() as i64;

    let context = TemplateContext {
        title: "역할 관리",
        active_page: "roles",
        user_id: user_id.0,
        current_user,
        roles: Some(roles),
        role: None,
        total,
        page,
        per_page: limit,
        total_pages,
        q: query.q,
        sort_by: query.sort_by,
        order: query.order,
        status: query.status,
    };

    templates::render_template(&state.tera, "pages/roles/list.html", context).await
}

pub async fn role_create_page(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
) -> Result<Response, AppError> {
    let current_user = state
        .service
        .user_service
        .get_user_by_id(user_id.0)
        .await
        .ok();

    let context = TemplateContext {
        title: "역할 생성",
        active_page: "roles",
        user_id: user_id.0,
        current_user,
        roles: None,
        role: None,
        total: 0,
        page: 1,
        per_page: 10,
        total_pages: 1,
        q: None,
        sort_by: None,
        order: None,
        status: None,
    };

    templates::render_template(&state.tera, "pages/roles/form.html", context).await
}

pub async fn role_edit_page(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let current_user = state
        .service
        .user_service
        .get_user_by_id(user_id.0)
        .await
        .ok();
    let role = state
        .service
        .role_service
        .get_role_by_id(id)
        .await?;

    let context = TemplateContext {
        title: "역할 수정",
        active_page: "roles",
        user_id: user_id.0,
        current_user,
        roles: None,
        role: Some(role),
        total: 0,
        page: 1,
        per_page: 10,
        total_pages: 1,
        q: None,
        sort_by: None,
        order: None,
        status: None,
    };

    templates::render_template(&state.tera, "pages/roles/form.html", context).await
}

pub async fn create_role(
    State(state): State<Arc<AppState>>,
    Extension(_user_id): Extension<UserId>,
    Form(payload): Form<CreateRoleRequest>,
) -> Result<Response, AppError> {
    state
        .service
        .role_service
        .create_role(payload)
        .await?;
    Ok(Redirect::to("/roles").into_response())
}

pub async fn update_role(
    State(state): State<Arc<AppState>>,
    Extension(_user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(payload): Form<UpdateRoleRequest>,
) -> Result<Response, AppError> {
    state
        .service
        .role_service
        .update_role(id, payload)
        .await?;
    Ok(Redirect::to("/roles").into_response())
}

pub async fn delete_role(
    State(state): State<Arc<AppState>>,
    Extension(_user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    state.service.role_service.delete_role(id).await?;
    Ok(Redirect::to("/roles").into_response())
}
