use serde_json::Value;
use sqlx::{
    Executor, Postgres,
    types::{Json, Uuid},
};

use crate::error::Result;

pub struct AuditEvent {
    pub actor_type: &'static str,
    pub actor_id: Option<Uuid>,
    pub event_type: &'static str,
    pub target_type: Option<&'static str>,
    pub target_id: Option<Uuid>,
    pub metadata: Value,
}

pub async fn record<'e, E>(executor: E, event: AuditEvent) -> Result<()>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into audit_logs (
            actor_type,
            actor_id,
            event_type,
            target_type,
            target_id,
            metadata
        )
        values ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(event.actor_type)
    .bind(event.actor_id)
    .bind(event.event_type)
    .bind(event.target_type)
    .bind(event.target_id)
    .bind(Json(event.metadata))
    .execute(executor)
    .await?;

    Ok(())
}
