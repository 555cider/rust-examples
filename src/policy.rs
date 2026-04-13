use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{
    FromRow, PgPool,
    types::{Json, Uuid},
};

use crate::error::Result;

#[derive(Debug, Deserialize)]
pub struct EvaluateRequest {
    pub subject: SubjectRef,
    pub resource: String,
    pub action: String,
    #[serde(default)]
    pub context: Value,
}

#[derive(Debug, Deserialize)]
pub struct SubjectRef {
    #[serde(rename = "type")]
    pub subject_type: String,
    pub id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    pub decision: &'static str,
    pub matched_rules: Vec<String>,
    pub reason: &'static str,
}

#[derive(Debug, FromRow)]
struct PolicyRule {
    rule_name: String,
    effect: String,
    resource_pattern: String,
    action_pattern: String,
    condition_expression: Json<Value>,
    priority: i32,
}

pub async fn evaluate(pool: &PgPool, request: EvaluateRequest) -> Result<EvaluateResponse> {
    let permission_name = format!("{}:{}", request.resource, request.action);
    let permissions =
        load_subject_permissions(pool, &request.subject.subject_type, request.subject.id).await?;
    let rbac_allowed = permissions
        .iter()
        .any(|permission| pattern_matches(permission, &permission_name));

    let mut matched_allow_rules = Vec::new();
    let mut matched_deny_rules = Vec::new();
    for rule in load_active_policy_rules(pool).await? {
        let _ = rule.priority;
        if !pattern_matches(&rule.resource_pattern, &request.resource) {
            continue;
        }
        if !pattern_matches(&rule.action_pattern, &request.action) {
            continue;
        }
        if !condition_matches(&rule.condition_expression.0, &request.context) {
            continue;
        }

        if rule.effect == "deny" {
            matched_deny_rules.push(rule.rule_name);
        } else {
            matched_allow_rules.push(rule.rule_name);
        }
    }

    if !matched_deny_rules.is_empty() {
        return Ok(EvaluateResponse {
            decision: "deny",
            matched_rules: matched_deny_rules,
            reason: "matched deny policy rule",
        });
    }
    if !matched_allow_rules.is_empty() {
        return Ok(EvaluateResponse {
            decision: "allow",
            matched_rules: matched_allow_rules,
            reason: "matched allow policy rule",
        });
    }
    if rbac_allowed {
        return Ok(EvaluateResponse {
            decision: "allow",
            matched_rules: Vec::new(),
            reason: "rbac permission matched",
        });
    }

    Ok(EvaluateResponse {
        decision: "deny",
        matched_rules: Vec::new(),
        reason: "no matching policy or permission",
    })
}

async fn load_subject_permissions(
    pool: &PgPool,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<Vec<String>> {
    let permissions = sqlx::query_scalar(
        r#"
        select permissions.permission_name
        from subject_role_bindings
        inner join role_permissions on role_permissions.role_id = subject_role_bindings.role_id
        inner join permissions on permissions.id = role_permissions.permission_id
        where subject_role_bindings.subject_type = $1
          and subject_role_bindings.subject_id = $2
        "#,
    )
    .bind(subject_type)
    .bind(subject_id)
    .fetch_all(pool)
    .await?;

    Ok(permissions)
}

async fn load_active_policy_rules(pool: &PgPool) -> Result<Vec<PolicyRule>> {
    let rules = sqlx::query_as(
        r#"
        select
            rule_name,
            effect,
            resource_pattern,
            action_pattern,
            condition_expression,
            priority
        from policy_rules
        where is_active = true
        order by priority desc, rule_name asc
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rules)
}

fn condition_matches(condition: &Value, context: &Value) -> bool {
    match condition {
        Value::Null => true,
        Value::Object(expected) if expected.is_empty() => true,
        Value::Object(expected) => {
            let Some(context_obj) = context.as_object() else {
                return false;
            };

            expected
                .iter()
                .all(|(key, value)| context_obj.get(key) == Some(value))
        }
        _ => false,
    }
}

fn pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }

    pattern == value
}
