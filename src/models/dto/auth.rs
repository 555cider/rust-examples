use crate::models::dto::role::RoleResponse;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct LoginRequest {
    #[validate(length(min = 1, message = "Username cannot be empty"))]
    pub username: String,
    #[validate(length(min = 8, message = "Password must be at least 8 characters long"))]
    pub password: String,
    pub redirect_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_url: Option<String>,
}

// 현재 로그인한 사용자 정보 응답 DTO
#[derive(Debug, Serialize, ToSchema)]
pub struct CurrentUserResponse {
    pub id: i64,
    pub username: String,
    pub role_id: i64,
    pub role: Option<RoleResponse>, // 사용자 종류 정보 포함 가능
    pub permissions: Vec<String>,            // 사용자 종류에 부여된 권한 코드 목록
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterRequest {
    #[validate(length(min = 1, message = "Username cannot be empty"))]
    pub username: String,
    #[validate(email(message = "Invalid email address"))]
    pub email: Option<String>,
    #[validate(length(min = 8, message = "Password must be at least 8 characters long"))]
    pub password: String,
    #[validate(range(min = 1, message = "Role cannot be empty"))]
    pub role_id: i64,
    pub is_active: Option<bool>,
}
