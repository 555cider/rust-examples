use crate::errors::AppError;
use axum::{async_trait, extract::FromRequestParts};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

/// Authenticated user with permissions loaded from the database.
/// This is the primary user type used throughout the application after authentication.
#[derive(Debug, Clone)]
pub struct AuthnUser {
    pub id: i64,
    pub role_id: i64,
    pub username: String,
    pub permissions: Arc<HashSet<String>>,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthnUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let authn_user = parts
            .extensions
            .get::<AuthnUser>()
            .ok_or(AppError::Unauthorized("Not authenticated".to_string()))?;
        Ok(authn_user.clone())
    }
}

/// Basic user model with role information.
/// This is a legacy type that may be deprecated in favor of AuthnUser.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password: String,
    pub role: String,
}

impl User {
    pub fn new(id: i64, username: String, password: String, role: String) -> Self {
        Self {
            id,
            username,
            password,
            role,
        }
    }

    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    pub fn is_user(&self) -> bool {
        self.role == "user"
    }
}
