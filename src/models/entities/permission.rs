use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Permission {
    pub id: Option<i64>,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl Permission {
    pub fn new(
        code: String,
        name: String,
        description: Option<String>,
        category: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            id: None,
            code,
            name,
            description,
            category,
            created_at: now,
            updated_at: now,
        }
    }
}
