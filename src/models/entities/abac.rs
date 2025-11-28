use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::time::Duration;

/// Core ABAC policy structure
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Policy {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub target_json: String, // JSON serialized Target
    pub rules_json: String,  // JSON serialized Vec<Rule>
    pub effect: PolicyEffect,
    pub priority: i32,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Policy effect enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyEffect {
    Permit,
    Deny,
}

impl std::fmt::Display for PolicyEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyEffect::Permit => write!(f, "PERMIT"),
            PolicyEffect::Deny => write!(f, "DENY"),
        }
    }
}

impl From<AttributeValue> for serde_json::Value {
    fn from(val: AttributeValue) -> Self {
        match val {
            AttributeValue::String(s) => serde_json::Value::String(s),
            AttributeValue::Number(n) => serde_json::json!(n),
            AttributeValue::Boolean(b) => serde_json::Value::Bool(b),
            AttributeValue::Array(arr) => {
                serde_json::Value::Array(arr.into_iter().map(Into::into).collect())
            }
            AttributeValue::Object(obj) => serde_json::Value::Object(
                obj.into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect(),
            ),
            AttributeValue::Null => serde_json::Value::Null,
        }
    }
}

impl std::str::FromStr for PolicyEffect {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "PERMIT" => Ok(PolicyEffect::Permit),
            "DENY" => Ok(PolicyEffect::Deny),
            _ => Err(format!("Invalid policy effect: {}", s)),
        }
    }
}

/// Policy target specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub subjects: Vec<String>,            // Subject attribute conditions
    pub resources: Vec<String>,           // Resource attribute conditions
    pub actions: Vec<String>,             // Action patterns
    pub environment: Option<Vec<String>>, // Environment conditions
}

/// Policy rule structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub condition: Condition,
    pub effect: PolicyEffect,
}

/// Condition structure for policy evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    #[serde(flatten)]
    pub expression: ConditionExpression,
}

/// Condition expression types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConditionExpression {
    Simple {
        attribute: String,
        operator: Operator,
        value: AttributeValue,
    },
    And {
        and: Vec<Condition>,
    },
    Or {
        or: Vec<Condition>,
    },
    Not {
        not: Box<Condition>,
    },
}

/// Comparison operators for conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operator {
    #[serde(rename = "==")]
    Equal,
    #[serde(rename = "!=")]
    NotEqual,
    #[serde(rename = ">")]
    GreaterThan,
    #[serde(rename = ">=")]
    GreaterThanOrEqual,
    #[serde(rename = "<")]
    LessThan,
    #[serde(rename = "<=")]
    LessThanOrEqual,
    #[serde(rename = "in")]
    In,
    #[serde(rename = "not_in")]
    NotIn,
    #[serde(rename = "contains")]
    Contains,
    #[serde(rename = "starts_with")]
    StartsWith,
    #[serde(rename = "ends_with")]
    EndsWith,
    #[serde(rename = "matches")]
    Matches, // Regex match
}

/// Attribute value types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<AttributeValue>),
    Object(HashMap<String, AttributeValue>),
    Null,
}

impl AttributeValue {
    /// Convert to string representation
    pub fn as_string(&self) -> Option<String> {
        match self {
            AttributeValue::String(s) => Some(s.clone()),
            AttributeValue::Number(n) => Some(n.to_string()),
            AttributeValue::Boolean(b) => Some(b.to_string()),
            _ => None,
        }
    }

    /// Convert to number representation
    pub fn as_number(&self) -> Option<f64> {
        match self {
            AttributeValue::Number(n) => Some(*n),
            AttributeValue::String(s) => s.parse().ok(),
            _ => None,
        }
    }

    /// Convert to boolean representation
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            AttributeValue::Boolean(b) => Some(*b),
            AttributeValue::String(s) => match s.to_lowercase().as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            },
            AttributeValue::Number(n) => Some(*n != 0.0),
            _ => None,
        }
    }

    /// Get the type name of the attribute value
    pub fn type_name(&self) -> &'static str {
        match self {
            AttributeValue::String(_) => "string",
            AttributeValue::Number(_) => "number",
            AttributeValue::Boolean(_) => "boolean",
            AttributeValue::Array(_) => "array",
            AttributeValue::Object(_) => "object",
            AttributeValue::Null => "null",
        }
    }
}

/// Access request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRequest {
    pub subject: Subject,
    pub resource: Resource,
    pub action: Action,
    pub context: Context,
}

/// Subject (user) information for access requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subject {
    pub id: String,
    pub attributes: HashMap<String, AttributeValue>,
}

/// Resource information for access requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub resource_type: String,
    pub attributes: HashMap<String, AttributeValue>,
}

/// Action information for access requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub name: String,
    pub attributes: Option<HashMap<String, AttributeValue>>,
}

/// Context information for access requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub environment: HashMap<String, AttributeValue>,
    pub request_time: DateTime<Utc>,
    pub session_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

/// Access response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessResponse {
    pub decision: Decision,
    pub applied_policies: Vec<AppliedPolicy>,
    pub evaluation_time: Duration,
    pub audit_info: AuditInfo,
}

/// Access decision enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    #[serde(rename = "PERMIT")]
    Permit,
    #[serde(rename = "DENY")]
    Deny,
    #[serde(rename = "NOT_APPLICABLE")]
    NotApplicable,
}

impl std::fmt::Display for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Decision::Permit => write!(f, "PERMIT"),
            Decision::Deny => write!(f, "DENY"),
            Decision::NotApplicable => write!(f, "NOT_APPLICABLE"),
        }
    }
}

/// Information about applied policies in the decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedPolicy {
    pub policy_id: String,
    pub policy_name: String,
    pub effect: PolicyEffect,
    pub matched_rules: Vec<String>,
}

/// Audit information for access decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditInfo {
    pub request_id: String,
    pub evaluation_method: EvaluationMethod,
    pub rbac_result: Option<Decision>,
    pub abac_result: Option<Decision>,
    pub final_decision: Decision,
    pub decision_reason: String,
}

/// Method used for evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvaluationMethod {
    #[serde(rename = "RBAC_ONLY")]
    RbacOnly,
    #[serde(rename = "ABAC_ONLY")]
    AbacOnly,
    #[serde(rename = "HYBRID")]
    Hybrid,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            environment: HashMap::new(),
            request_time: Utc::now(),
            session_id: None,
            ip_address: None,
            user_agent: None,
        }
    }
}

impl Policy {
    /// Parse the target JSON into a Target struct
    pub fn parse_target(&self) -> Result<Target, serde_json::Error> {
        serde_json::from_str(&self.target_json)
    }

    /// Parse the rules JSON into a Vec<Rule>
    pub fn parse_rules(&self) -> Result<Vec<Rule>, serde_json::Error> {
        serde_json::from_str(&self.rules_json)
    }

    /// Create a new policy with JSON serialization
    pub fn new(
        id: String,
        name: String,
        description: Option<String>,
        target: Target,
        rules: Vec<Rule>,
        effect: PolicyEffect,
        priority: i32,
    ) -> Result<Self, serde_json::Error> {
        let target_json = serde_json::to_string(&target)?;
        let rules_json = serde_json::to_string(&rules)?;
        let now = Utc::now().naive_utc();

        Ok(Self {
            id,
            name,
            description,
            target_json,
            rules_json,
            effect,
            priority,
            is_active: true,
            created_at: now,
            updated_at: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_value_conversions() {
        let string_val = AttributeValue::String("test".to_string());
        assert_eq!(string_val.as_string(), Some("test".to_string()));
        assert_eq!(string_val.type_name(), "string");

        let number_val = AttributeValue::Number(42.0);
        assert_eq!(number_val.as_number(), Some(42.0));
        assert_eq!(number_val.type_name(), "number");

        let bool_val = AttributeValue::Boolean(true);
        assert_eq!(bool_val.as_boolean(), Some(true));
        assert_eq!(bool_val.type_name(), "boolean");
    }

    #[test]
    fn test_policy_creation() {
        let target = Target {
            subjects: vec!["user.department == 'IT'".to_string()],
            resources: vec!["resource.type == 'server'".to_string()],
            actions: vec!["read".to_string(), "write".to_string()],
            environment: None,
        };

        let condition = Condition {
            expression: ConditionExpression::Simple {
                attribute: "user.role".to_string(),
                operator: Operator::Equal,
                value: AttributeValue::String("admin".to_string()),
            },
        };

        let rules = vec![Rule {
            condition,
            effect: PolicyEffect::Permit,
        }];

        let policy = Policy::new(
            "test-policy".to_string(),
            "Test Policy".to_string(),
            Some("A test policy".to_string()),
            target,
            rules,
            PolicyEffect::Permit,
            100,
        );

        assert!(policy.is_ok());
        let policy = policy.unwrap();
        assert_eq!(policy.id, "test-policy");
        assert_eq!(policy.name, "Test Policy");
        assert!(policy.parse_target().is_ok());
        assert!(policy.parse_rules().is_ok());
    }

    #[test]
    fn test_condition_serialization() {
        let condition = Condition {
            expression: ConditionExpression::And {
                and: vec![
                    Condition {
                        expression: ConditionExpression::Simple {
                            attribute: "user.department".to_string(),
                            operator: Operator::Equal,
                            value: AttributeValue::String("IT".to_string()),
                        },
                    },
                    Condition {
                        expression: ConditionExpression::Simple {
                            attribute: "resource.sensitivity".to_string(),
                            operator: Operator::LessThan,
                            value: AttributeValue::Number(5.0),
                        },
                    },
                ],
            },
        };

        let json = serde_json::to_string(&condition);
        assert!(json.is_ok());

        let deserialized: Result<Condition, _> = serde_json::from_str(&json.unwrap());
        assert!(deserialized.is_ok());
    }
}
