use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// Access evaluation log entry representing permission evaluation results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessEvaluationLog {
    pub id: i64,
    pub request_id: String,
    pub user_id: Option<i64>,
    pub resource_id: Option<String>,
    pub resource_type: Option<String>,
    pub action: String,
    pub decision: AccessDecision,
    pub rbac_result: Option<AccessDecision>,
    pub abac_result: Option<AccessDecision>,
    #[serde(
        serialize_with = "serialize_applied_policies",
        deserialize_with = "deserialize_applied_policies"
    )]
    pub applied_policies: Option<String>, // JSON array of policy IDs
    pub evaluation_time_ms: Option<i64>,
    #[serde(
        rename = "context",
        serialize_with = "serialize_context",
        deserialize_with = "deserialize_context"
    )]
    pub context_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Access decision enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AccessDecision {
    Permit,
    Deny,
    NotApplicable,
}

impl std::fmt::Display for AccessDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccessDecision::Permit => write!(f, "PERMIT"),
            AccessDecision::Deny => write!(f, "DENY"),
            AccessDecision::NotApplicable => write!(f, "NOT_APPLICABLE"),
        }
    }
}

impl std::str::FromStr for AccessDecision {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "PERMIT" => Ok(AccessDecision::Permit),
            "DENY" => Ok(AccessDecision::Deny),
            "NOT_APPLICABLE" => Ok(AccessDecision::NotApplicable),
            _ => Err(format!("Invalid access decision: {}", s)),
        }
    }
}

impl AccessEvaluationLog {
    /// Creates a new access evaluation log entry
    pub fn new(
        request_id: String,
        user_id: Option<i64>,
        resource_id: Option<String>,
        resource_type: Option<String>,
        action: String,
        decision: AccessDecision,
        rbac_result: Option<AccessDecision>,
        abac_result: Option<AccessDecision>,
        applied_policies: Option<Vec<String>>,
        evaluation_time_ms: Option<i64>,
        context: Option<Value>,
    ) -> Self {
        Self {
            id: 0, // Will be set by the database
            request_id,
            user_id,
            resource_id,
            resource_type,
            action,
            decision,
            rbac_result,
            abac_result,
            applied_policies: applied_policies
                .map(|policies| serde_json::to_string(&policies).unwrap_or_default()),
            evaluation_time_ms,
            context_json: context.map(|v| v.to_string()),
            created_at: Utc::now(),
        }
    }

    /// Gets the applied policies as a vector of strings
    pub fn get_applied_policies(&self) -> Vec<String> {
        self.applied_policies
            .as_ref()
            .and_then(|policies| serde_json::from_str(policies).ok())
            .unwrap_or_default()
    }

    /// Gets the context as a JSON value
    pub fn get_context(&self) -> Option<Value> {
        self.context_json
            .as_ref()
            .and_then(|ctx| serde_json::from_str(ctx).ok())
    }
}

/// Performance metrics for access evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessEvaluationMetrics {
    pub total_evaluations: i64,
    pub permit_count: i64,
    pub deny_count: i64,
    pub not_applicable_count: i64,
    pub average_evaluation_time_ms: f64,
    pub max_evaluation_time_ms: i64,
    pub min_evaluation_time_ms: i64,
    pub rbac_only_count: i64,
    pub abac_only_count: i64,
    pub hybrid_count: i64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

/// Filter criteria for access evaluation log queries
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccessEvaluationLogFilter {
    pub user_id: Option<i64>,
    pub resource_type: Option<String>,
    pub action: Option<String>,
    pub decision: Option<AccessDecision>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// Custom serializers for applied_policies to parse JSON array into Vec
fn serialize_applied_policies<S>(
    value: &Option<String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(json_str) => {
            let parsed: Result<Vec<String>, _> = serde_json::from_str(json_str);
            match parsed {
                Ok(vec) => vec.serialize(serializer),
                Err(_) => serializer.serialize_none(),
            }
        }
        None => serializer.serialize_none(),
    }
}

fn deserialize_applied_policies<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<Vec<String>> = Option::deserialize(deserializer)?;
    Ok(opt.and_then(|vec| serde_json::to_string(&vec).ok()))
}

// Custom serializers for context to parse JSON object into Value
fn serialize_context<S>(value: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(json_str) => {
            let parsed: Result<Value, _> = serde_json::from_str(json_str);
            match parsed {
                Ok(val) => val.serialize(serializer),
                Err(_) => serializer.serialize_none(),
            }
        }
        None => serializer.serialize_none(),
    }
}

fn deserialize_context<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<Value> = Option::deserialize(deserializer)?;
    Ok(opt.map(|val| val.to_string()))
}
