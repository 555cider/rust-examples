use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

/// Request for single access control evaluation
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct AccessControlRequest {
    /// Subject (user) identifier
    #[validate(length(min = 1, message = "Subject cannot be empty"))]
    pub subject: String,

    /// Resource identifier
    #[validate(length(min = 1, message = "Resource cannot be empty"))]
    pub resource: String,

    /// Resource type (e.g., "document", "server", "api")
    pub resource_type: Option<String>,

    /// Action to perform (e.g., "read", "write", "delete")
    #[validate(length(min = 1, message = "Action cannot be empty"))]
    pub action: String,

    /// Additional context attributes
    pub context: Option<HashMap<String, serde_json::Value>>,
}

/// Request for batch access control evaluation
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct BatchAccessControlRequest {
    /// Subject (user) identifier
    #[validate(length(min = 1, message = "Subject cannot be empty"))]
    pub subject: String,

    /// List of resource access requests
    #[validate(length(min = 1, message = "At least one resource request is required"))]
    pub requests: Vec<ResourceAccessRequest>,

    /// Additional context attributes
    pub context: Option<HashMap<String, serde_json::Value>>,
}

/// Individual resource access request within a batch
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResourceAccessRequest {
    /// Resource identifier
    #[validate(length(min = 1, message = "Resource cannot be empty"))]
    pub resource: String,

    /// Resource type
    pub resource_type: Option<String>,

    /// Action to perform
    #[validate(length(min = 1, message = "Action cannot be empty"))]
    pub action: String,
}

/// Response for access control evaluation
#[derive(Debug, Clone, Serialize)]
pub struct AccessControlResponse {
    /// Whether access is granted
    pub granted: bool,

    /// Decision details
    pub decision: AccessDecision,

    /// Applied policies and rules
    pub applied_policies: Vec<AppliedPolicy>,

    /// Evaluation time in milliseconds
    pub evaluation_time_ms: u64,

    /// Request ID for audit tracking
    pub request_id: String,

    /// Timestamp of evaluation
    pub evaluated_at: DateTime<Utc>,
}

/// Response for batch access control evaluation
#[derive(Debug, Clone, Serialize)]
pub struct BatchAccessControlResponse {
    /// Individual access control results
    pub results: Vec<ResourceAccessResult>,

    /// Total evaluation time in milliseconds
    pub total_evaluation_time_ms: u64,

    /// Request ID for audit tracking
    pub request_id: String,

    /// Timestamp of evaluation
    pub evaluated_at: DateTime<Utc>,
}

/// Individual resource access result within a batch response
#[derive(Debug, Clone, Serialize)]
pub struct ResourceAccessResult {
    /// Resource identifier
    pub resource: String,

    /// Resource type
    pub resource_type: Option<String>,

    /// Action
    pub action: String,

    /// Whether access is granted
    pub granted: bool,

    /// Decision details
    pub decision: AccessDecision,

    /// Applied policies
    pub applied_policies: Vec<AppliedPolicy>,

    /// Individual evaluation time in milliseconds
    pub evaluation_time_ms: u64,
}

/// Access decision details
#[derive(Debug, Clone, Serialize)]
pub struct AccessDecision {
    /// Final decision
    pub decision: Decision,

    /// RBAC evaluation result
    pub rbac_result: Option<RbacResult>,

    /// ABAC evaluation result
    pub abac_result: Option<AbacResult>,

    /// Combination strategy used
    pub combination_strategy: String,

    /// Reason for the decision
    pub reason: Option<String>,
}

/// RBAC evaluation result
#[derive(Debug, Clone, Serialize)]
pub struct RbacResult {
    /// Whether RBAC granted access
    pub granted: bool,

    /// Applied roles
    pub applied_roles: Vec<String>,

    /// Applied permissions
    pub applied_permissions: Vec<String>,

    /// Evaluation time in milliseconds
    pub evaluation_time_ms: u64,
}

/// ABAC evaluation result
#[derive(Debug, Clone, Serialize)]
pub struct AbacResult {
    /// Whether ABAC granted access
    pub granted: bool,

    /// Applied policies
    pub applied_policies: Vec<String>,

    /// Evaluation details
    pub evaluation_details: Vec<PolicyEvaluationDetail>,

    /// Evaluation time in milliseconds
    pub evaluation_time_ms: u64,
}

/// Policy evaluation detail
#[derive(Debug, Clone, Serialize)]
pub struct PolicyEvaluationDetail {
    /// Policy ID
    pub policy_id: String,

    /// Policy name
    pub policy_name: String,

    /// Whether policy was applicable
    pub applicable: bool,

    /// Policy decision
    pub decision: Decision,

    /// Rule evaluation results
    pub rule_results: Vec<RuleEvaluationResult>,
}

/// Rule evaluation result
#[derive(Debug, Clone, Serialize)]
pub struct RuleEvaluationResult {
    /// Rule description
    pub rule_description: Option<String>,

    /// Whether rule condition was satisfied
    pub condition_satisfied: bool,

    /// Rule effect
    pub effect: String,

    /// Condition details
    pub condition_details: Option<serde_json::Value>,
}

/// Applied policy information
#[derive(Debug, Clone, Serialize)]
pub struct AppliedPolicy {
    /// Policy type (RBAC or ABAC)
    pub policy_type: PolicyType,

    /// Policy identifier
    pub policy_id: String,

    /// Policy name
    pub policy_name: String,

    /// Policy effect
    pub effect: String,

    /// Priority
    pub priority: Option<i32>,
}

/// Decision enum
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Decision {
    #[serde(rename = "PERMIT")]
    Permit,
    #[serde(rename = "DENY")]
    Deny,
    #[serde(rename = "NOT_APPLICABLE")]
    NotApplicable,
}

/// Policy type enum
#[derive(Debug, Clone, Serialize)]
pub enum PolicyType {
    #[serde(rename = "RBAC")]
    Rbac,
    #[serde(rename = "ABAC")]
    Abac,
}

/// Legacy permission check request for backward compatibility
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct LegacyPermissionCheckRequest {
    /// User ID
    pub user_id: i64,

    /// Permission code
    #[validate(length(min = 1, message = "Permission code cannot be empty"))]
    pub permission_code: String,

    /// Optional resource context
    pub resource: Option<String>,
}

/// Legacy permission check response
#[derive(Debug, Clone, Serialize)]
pub struct LegacyPermissionCheckResponse {
    /// Whether permission is granted
    pub granted: bool,

    /// Permission details
    pub permission: Option<String>,

    /// User roles
    pub roles: Vec<String>,
}

impl From<AccessControlResponse> for LegacyPermissionCheckResponse {
    fn from(response: AccessControlResponse) -> Self {
        let roles = response
            .applied_policies
            .iter()
            .filter(|p| matches!(p.policy_type, PolicyType::Rbac))
            .map(|p| p.policy_name.clone())
            .collect();

        Self {
            granted: response.granted,
            permission: Some(response.decision.reason.unwrap_or_default()),
            roles,
        }
    }
}
