use crate::models::entities::role::Role;
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateRoleRequest {
    #[validate(length(
        min = 1,
        max = 50,
        message = "Code must be between 1 and 50 characters"
    ))]
    pub code: String,

    #[validate(length(
        min = 1,
        max = 100,
        message = "Name must be between 1 and 100 characters"
    ))]
    pub name: String,

    #[validate(length(max = 255, message = "Description cannot exceed 255 characters"))]
    pub description: Option<String>,

    #[serde(default = "default_is_active")]
    pub is_active: bool,

    pub permissions: Option<Vec<i64>>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateRoleRequest {
    #[validate(length(
        min = 1,
        max = 50,
        message = "Code must be between 1 and 50 characters"
    ))]
    pub code: Option<String>,

    #[validate(length(
        min = 1,
        max = 100,
        message = "Name must be between 1 and 100 characters"
    ))]
    pub name: Option<String>,

    #[validate(length(max = 255, message = "Description cannot exceed 255 characters"))]
    pub description: Option<String>,

    pub is_active: Option<bool>,

    pub permissions: Option<Vec<i64>>,
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct RoleResponse {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub permissions: Option<Vec<i64>>,
}

impl From<Role> for RoleResponse {
    fn from(ut: Role) -> Self {
        Self {
            id: ut.id,
            code: ut.code,
            name: ut.name,
            description: ut.description,
            is_active: ut.is_active,
            created_at: Utc.from_utc_datetime(&ut.created_at),
            updated_at: Utc.from_utc_datetime(&ut.updated_at),
            permissions: None, // This will be populated separately if needed
        }
    }
}

fn default_is_active() -> bool {
    true
}
