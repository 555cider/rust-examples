use crate::models::entities::admin_user::AdminUser;
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateUserRequest {
    #[validate(length(min = 3, message = "Username must be at least 3 characters long"))]
    pub username: String,
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
    #[validate(length(min = 8, message = "Password must be at least 8 characters long"))]
    pub password: String,
    #[validate(range(min = 1, message = "Invalid role ID"))]
    pub role_id: i64,
    pub is_active: Option<bool>, // 생성 시 선택적 활성화
    pub attributes: Option<String>, // JSON string for ABAC attributes
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateUserRequest {
    // 유효성 검사: 모든 필드가 Option이므로,至少 하나는 있어야 한다는 커스텀 검증 필요 가능
    #[validate(length(min = 3, message = "Username must be at least 3 characters long"))]
    pub username: Option<String>,
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    #[validate(range(min = 1, message = "Invalid role ID"))]
    pub role_id: Option<i64>,
    pub is_active: Option<bool>,
    pub attributes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub email: Option<String>,
    pub role_id: i64,
    pub is_active: bool,
    pub attributes: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// 모델 -> 응답 DTO 변환
impl From<AdminUser> for UserResponse {
    fn from(user: AdminUser) -> Self {
        Self {
            id: user.id,
            username: user.username,
            email: user.email,
            role_id: user.role_id,
            is_active: user.is_active,
            attributes: user.attributes,
            last_login_at: user.last_login_at.map(|ndt| Utc.from_utc_datetime(&ndt)),
            created_at: Utc.from_utc_datetime(&user.created_at),
            updated_at: Utc.from_utc_datetime(&user.updated_at),
        }
    }
}
