use crate::{
    errors::AppError,
    models::dto::common::{ListQueryParams, StringIdQuery},
    AppState,
};
use axum::{
    extract::{Json, Query, State, Extension},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Router,
};
use crate::middleware::auth::UserId;
use crate::services::access_control::{AccessControlRequest, AccessContext, AccessControlDecision};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use validator::Validate;

/// Helper function to extract and deserialize a JSON field
fn extract_json_field<T: Default + serde::de::DeserializeOwned>(
    json: &serde_json::Value,
    field: &str,
) -> T {
    json.get(field)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Helper function to extract an optional JSON field
fn extract_json_field_optional<T: serde::de::DeserializeOwned>(
    json: &serde_json::Value,
    field: &str,
) -> Option<T> {
    json.get(field)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Parse policy target from JSON
fn parse_policy_target(policy_json: &serde_json::Value) -> crate::models::entities::abac::Target {
    crate::models::entities::abac::Target {
        subjects: extract_json_field(policy_json, "subjects"),
        resources: extract_json_field(policy_json, "resources"),
        actions: extract_json_field(policy_json, "actions"),
        environment: extract_json_field_optional(policy_json, "environment"),
    }
}

/// Parse policy rules from JSON
fn parse_policy_rules(policy_json: &serde_json::Value) -> Vec<crate::models::entities::abac::Rule> {
    match policy_json.get("rules") {
        Some(v) => match serde_json::from_value(v.clone()) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to parse rules: {}", e);
                vec![]
            }
        },
        None => {
            tracing::warn!("No rules found in policy JSON");
            vec![]
        },
    }
}

/// Helper function to check policy permissions
async fn check_policy_permission(
    manager: Option<&Arc<crate::services::access_control::AccessControlManager>>,
    user_id: i64,
    resource: &str,
    action: &str,
) -> Result<(), AppError> {
    if let Some(manager) = manager {
        let access_req = AccessControlRequest {
            request_id: None,
            user_id,
            resource: resource.to_string(),
            action: action.to_string(),
            resource_type: Some("policy".to_string()),
            context: AccessContext::default(),
        };

        let decision = manager.evaluate_access(&access_req).await
            .map_err(|e| AppError::InternalServerError(format!("Access control evaluation failed: {}", e)))?;

        if decision.decision == AccessControlDecision::Deny {
            let error_msg = match action {
                "create" => "You do not have permission to create policies",
                "update" => "You do not have permission to update this policy",
                "delete" => "You do not have permission to delete this policy",
                _ => "You do not have permission to perform this action",
            };
            return Err(AppError::Forbidden(error_msg.to_string()));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreatePolicyRequest {
    #[validate(length(min = 1, message = "Policy ID cannot be empty"))]
    pub id: String,

    #[validate(length(min = 1, message = "Policy name cannot be empty"))]
    pub name: String,

    pub description: Option<String>,

    /// JSON policy definition
    #[validate(length(min = 1, message = "Policy definition cannot be empty"))]
    pub policy_json: String,

    pub priority: Option<i32>,

    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdatePolicyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub policy_json: Option<String>,
    pub priority: Option<i32>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub policy_json: serde_json::Value,
    pub rules: Option<serde_json::Value>,
    pub priority: i32,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyTestRequest {
    pub policy_json: String,
    pub test_cases: Vec<PolicyTestCase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyTestCase {
    pub name: String,
    #[allow(dead_code)]
    pub subject_attributes: std::collections::HashMap<String, serde_json::Value>,
    #[allow(dead_code)]
    pub resource_attributes: std::collections::HashMap<String, serde_json::Value>,
    #[allow(dead_code)]
    pub environment_attributes: std::collections::HashMap<String, serde_json::Value>,
    #[allow(dead_code)]
    pub action: String,
    pub expected_decision: String, // "PERMIT", "DENY", "NOT_APPLICABLE"
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyTestResponse {
    pub policy_valid: bool,
    pub validation_errors: Vec<String>,
    pub test_results: Vec<PolicyTestResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyTestResult {
    pub test_case_name: String,
    pub expected_decision: String,
    pub actual_decision: String,
    pub passed: bool,
    pub evaluation_details: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyValidationRequest {
    pub policy_json: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub parsed_policy: Option<serde_json::Value>,
}

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_policies).post(create_policy))
        .route("/detail", get(get_policy_detail))
        .route("/update", put(update_policy))
        .route("/delete", delete(delete_policy))
        .route("/activate", post(activate_policy))
        .route("/deactivate", post(deactivate_policy))
        .route("/toggle", post(toggle_policy))
        .route("/validate", post(validate_policy))
        .route("/test", post(test_policy))
        .layer(middleware::from_fn(crate::middleware::auth))
}

/// Create a new ABAC policy
async fn create_policy(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
    Json(req): Json<CreatePolicyRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Check Access Control
    check_policy_permission(
        state.service.access_control_manager.as_ref(),
        user_id.0,
        "policy",
        "create",
    ).await?;

    // Validate the request
    req.validate()
        .map_err(|e| AppError::BadRequest(format!("Validation error: {}", e)))?;

    // Validate policy JSON format
    let policy_json: serde_json::Value = serde_json::from_str(&req.policy_json)
        .map_err(|e| AppError::BadRequest(format!("Invalid policy JSON: {}", e)))?;

    // Get policy repository through service container
    let policy_repo = &state.service.policy_repository;

    // Extract effect from policy JSON
    let effect = policy_json
        .get("effect")
        .and_then(|e| e.as_str())
        .and_then(|e| e.parse::<crate::models::entities::abac::PolicyEffect>().ok())
        .unwrap_or(crate::models::entities::abac::PolicyEffect::Permit);

    // Extract target fields using helper function
    let target = parse_policy_target(&policy_json);

    // Extract rules using helper function
    let rules = parse_policy_rules(&policy_json);

    let target_json_str = serde_json::to_string(&target)
        .map_err(|e| AppError::InternalServerError(format!("Failed to serialize target: {}", e)))?;
    
    let rules_json_str = serde_json::to_string(&rules)
        .map_err(|e| AppError::InternalServerError(format!("Failed to serialize rules: {}", e)))?;

    // Create policy entity
    let policy = crate::models::entities::abac::Policy {
        id: req.id,
        name: req.name,
        description: req.description,
        target_json: target_json_str,
        rules_json: rules_json_str,
        effect,
        priority: req.priority.unwrap_or(0),
        is_active: req.is_active.unwrap_or(true),
        created_at: chrono::Utc::now().naive_utc(),
        updated_at: chrono::Utc::now().naive_utc(),
    };

    // Validate policy JSON structure before saving
    policy_repo.validate_policy_json(&policy).await?;

    // Create policy in repository
    let policy_id = policy_repo.create(policy.clone()).await?;

    let response = PolicyResponse {
        id: policy_id,
        name: policy.name,
        description: policy.description,
        policy_json,
        rules: Some(serde_json::to_value(rules).unwrap_or(serde_json::json!([]))),
        priority: policy.priority,
        is_active: policy.is_active,
        created_at: chrono::DateTime::from_naive_utc_and_offset(policy.created_at, chrono::Utc),
        updated_at: chrono::DateTime::from_naive_utc_and_offset(policy.updated_at, chrono::Utc),
    };

    Ok((StatusCode::CREATED, Json(response)).into_response())
}

/// List all policies with optional filtering
async fn list_policies(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    let policy_repo = &state.service.policy_repository;

    let is_active = query.status.as_deref().map(|s| s == "active");

    // Query policies from repository
    let policies_entities = policy_repo
        .find_all(
            query.q.clone(),
            is_active,
            Some(query.get_limit()),
            Some(query.get_offset()),
        )
        .await?;

    // Convert to response DTOs
    let policies: Vec<PolicyResponse> = policies_entities
        .into_iter()
        .map(|p| {
            let policy_json = serde_json::from_str(&p.target_json).unwrap_or(serde_json::json!({}));
            let rules = serde_json::from_str(&p.rules_json).ok();
            PolicyResponse {
                id: p.id,
                name: p.name,
                description: p.description,
                policy_json,
                rules,
                priority: p.priority,
                is_active: p.is_active,
                created_at: chrono::DateTime::from_naive_utc_and_offset(p.created_at, chrono::Utc),
                updated_at: chrono::DateTime::from_naive_utc_and_offset(p.updated_at, chrono::Utc),
            }
        })
        .collect();

    // Get total count
    let total = policy_repo.count(query.q.clone(), is_active).await?;

    // Create paginated response
    let response = serde_json::json!({
        "items": policies,
        "total": total,
        "page": query.page.unwrap_or(1),
        "limit": query.get_limit(),
    });

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Get a specific policy by ID
async fn get_policy_detail(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StringIdQuery>,
) -> Result<Json<PolicyResponse>, AppError> {
    let policy_repo = &state.service.policy_repository;

    // Query policy from repository
    let policy = policy_repo.find_by_id(&query.id).await?;

    let policy_json = serde_json::from_str(&policy.target_json).unwrap_or(serde_json::json!({}));
    let rules = serde_json::from_str(&policy.rules_json).ok();

    let response = PolicyResponse {
        id: policy.id,
        name: policy.name,
        description: policy.description,
        policy_json,
        rules,
        priority: policy.priority,
        is_active: policy.is_active,
        created_at: chrono::DateTime::from_naive_utc_and_offset(policy.created_at, chrono::Utc),
        updated_at: chrono::DateTime::from_naive_utc_and_offset(policy.updated_at, chrono::Utc),
    };

    Ok(Json(response))
}

/// Update an existing policy
async fn update_policy(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
    Query(query): Query<StringIdQuery>,
    Json(req): Json<UpdatePolicyRequest>,
) -> Result<Json<PolicyResponse>, AppError> {
    // Check Access Control
    check_policy_permission(
        state.service.access_control_manager.as_ref(),
        user_id.0,
        &query.id,
        "update",
    ).await?;

    let policy_repo = &state.service.policy_repository;
    let id = query.id;

    // Get existing policy
    let mut policy = policy_repo.find_by_id(&id).await?;

    // Update fields if provided
    if let Some(name) = req.name {
        policy.name = name;
    }
    if let Some(description) = req.description {
        policy.description = Some(description);
    }
    if let Some(policy_json_str) = req.policy_json {
        // Validate JSON format
        let policy_json: serde_json::Value = serde_json::from_str(&policy_json_str)
            .map_err(|e| AppError::BadRequest(format!("Invalid policy JSON: {}", e)))?;

        // Extract target fields using helper function
        let target = parse_policy_target(&policy_json);

        // Extract rules using helper function
        let rules = parse_policy_rules(&policy_json);

        let target_json_str = serde_json::to_string(&target)
            .map_err(|e| AppError::InternalServerError(format!("Failed to serialize target: {}", e)))?;
        
        let rules_json_str = serde_json::to_string(&rules)
            .map_err(|e| AppError::InternalServerError(format!("Failed to serialize rules: {}", e)))?;

        policy.target_json = target_json_str;
        policy.rules_json = rules_json_str;
    }
    if let Some(priority) = req.priority {
        policy.priority = priority;
    }
    if let Some(is_active) = req.is_active {
        policy.is_active = is_active;
    }

    // Update in repository
    let updated_policy = policy_repo.update(&id, policy).await?;

    let policy_json =
        serde_json::from_str(&updated_policy.target_json).unwrap_or(serde_json::json!({}));
    let rules = serde_json::from_str(&updated_policy.rules_json).ok();

    let response = PolicyResponse {
        id: updated_policy.id,
        name: updated_policy.name,
        description: updated_policy.description,
        policy_json,
        rules,
        priority: updated_policy.priority,
        is_active: updated_policy.is_active,
        created_at: chrono::DateTime::from_naive_utc_and_offset(
            updated_policy.created_at,
            chrono::Utc,
        ),
        updated_at: chrono::DateTime::from_naive_utc_and_offset(
            updated_policy.updated_at,
            chrono::Utc,
        ),
    };

    Ok(Json(response))
}

/// Delete a policy
async fn delete_policy(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
    Query(query): Query<StringIdQuery>,
) -> Result<StatusCode, AppError> {
    // Check Access Control
    check_policy_permission(
        state.service.access_control_manager.as_ref(),
        user_id.0,
        &query.id,
        "delete",
    ).await?;

    let policy_repo = &state.service.policy_repository;

    // Soft delete the policy (sets is_active to false)
    policy_repo.soft_delete(&query.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Activate a policy
async fn activate_policy(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StringIdQuery>,
) -> Result<StatusCode, AppError> {
    let policy_repo = &state.service.policy_repository;

    // Get existing policy and activate it
    let mut policy = policy_repo.find_by_id(&query.id).await?;
    policy.is_active = true;
    policy_repo.update(&query.id, policy).await?;

    Ok(StatusCode::OK)
}

/// Deactivate a policy
async fn deactivate_policy(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StringIdQuery>,
) -> Result<StatusCode, AppError> {
    let policy_repo = &state.service.policy_repository;

    // Get existing policy and deactivate it
    let mut policy = policy_repo.find_by_id(&query.id).await?;
    policy.is_active = false;
    policy_repo.update(&query.id, policy).await?;

    Ok(StatusCode::OK)
}

/// Toggle a policy's active status
async fn toggle_policy(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<UserId>,
    Query(query): Query<StringIdQuery>,
) -> Result<Json<PolicyResponse>, AppError> {
    // Check Access Control
    check_policy_permission(
        state.service.access_control_manager.as_ref(),
        user_id.0,
        &query.id,
        "update",
    ).await?;

    let policy_repo = &state.service.policy_repository;
    let id = query.id;

    // Get existing policy and toggle its active status
    let mut policy = policy_repo.find_by_id(&id).await?;
    policy.is_active = !policy.is_active;
    let updated_policy = policy_repo.update(&id, policy).await?;

    let policy_json =
        serde_json::from_str(&updated_policy.target_json).unwrap_or(serde_json::json!({}));
    let rules = serde_json::from_str(&updated_policy.rules_json).ok();

    let response = PolicyResponse {
        id: updated_policy.id,
        name: updated_policy.name,
        description: updated_policy.description,
        policy_json,
        rules,
        priority: updated_policy.priority,
        is_active: updated_policy.is_active,
        created_at: chrono::DateTime::from_naive_utc_and_offset(
            updated_policy.created_at,
            chrono::Utc,
        ),
        updated_at: chrono::DateTime::from_naive_utc_and_offset(
            updated_policy.updated_at,
            chrono::Utc,
        ),
    };

    Ok(Json(response))
}

/// Validate a policy without saving it
async fn validate_policy(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<PolicyValidationRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut parsed_policy = None;

    // Basic JSON validation
    match serde_json::from_str::<serde_json::Value>(&req.policy_json) {
        Ok(json) => {
            parsed_policy = Some(json.clone());

            // Validate policy structure
            if let Err(validation_errors) = validate_policy_structure(&json) {
                errors.extend(validation_errors);
            }

            // Check for potential issues
            warnings.extend(check_policy_warnings(&json));
        }
        Err(e) => {
            errors.push(format!("Invalid JSON: {}", e));
        }
    }

    let response = PolicyValidationResponse {
        valid: errors.is_empty(),
        errors,
        warnings,
        parsed_policy,
    };

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Test a policy against test cases
async fn test_policy(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<PolicyTestRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut validation_errors = Vec::new();
    let mut test_results = Vec::new();

    // First validate the policy
    let policy_json = match serde_json::from_str::<serde_json::Value>(&req.policy_json) {
        Ok(json) => {
            if let Err(errors) = validate_policy_structure(&json) {
                validation_errors.extend(errors);
                json
            } else {
                json
            }
        }
        Err(e) => {
            validation_errors.push(format!("Invalid JSON: {}", e));
            return Ok((
                StatusCode::OK,
                Json(PolicyTestResponse {
                    policy_valid: false,
                    validation_errors,
                    test_results,
                }),
            )
                .into_response());
        }
    };

    // Run test cases if policy is valid
    if validation_errors.is_empty() {
        for test_case in req.test_cases {
            let result = run_policy_test_case(&policy_json, &test_case).await;
            test_results.push(result);
        }
    }

    let response = PolicyTestResponse {
        policy_valid: validation_errors.is_empty(),
        validation_errors,
        test_results,
    };

    Ok((StatusCode::OK, Json(response)).into_response())
}

// Helper functions

fn validate_policy_structure(policy_json: &serde_json::Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    // Check required fields
    if !policy_json.is_object() {
        errors.push("Policy must be a JSON object".to_string());
        return Err(errors);
    }

    let policy_obj = policy_json.as_object().unwrap();

    // Check for required fields
    if !policy_obj.contains_key("id") {
        errors.push("Policy must have an 'id' field".to_string());
    }

    if !policy_obj.contains_key("name") {
        errors.push("Policy must have a 'name' field".to_string());
    }

    if !policy_obj.contains_key("rules") {
        errors.push("Policy must have a 'rules' field".to_string());
    } else if !policy_obj["rules"].is_array() {
        errors.push("Policy 'rules' field must be an array".to_string());
    }

    if !policy_obj.contains_key("effect") {
        errors.push("Policy must have an 'effect' field".to_string());
    } else {
        let effect = policy_obj["effect"].as_str().unwrap_or("");
        if !["PERMIT", "DENY"].contains(&effect) {
            errors.push("Policy 'effect' must be either 'PERMIT' or 'DENY'".to_string());
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_policy_warnings(policy_json: &serde_json::Value) -> Vec<String> {
    let mut warnings = Vec::new();

    if let Some(policy_obj) = policy_json.as_object() {
        // Check for missing optional but recommended fields
        if !policy_obj.contains_key("description") {
            warnings
                .push("Consider adding a 'description' field for better documentation".to_string());
        }

        if !policy_obj.contains_key("priority") {
            warnings.push("Consider adding a 'priority' field for policy ordering".to_string());
        }

        // Check rules complexity
        if let Some(rules) = policy_obj.get("rules").and_then(|r| r.as_array()) {
            if rules.len() > 10 {
                warnings.push(
                    "Policy has many rules, consider splitting into multiple policies".to_string(),
                );
            }
        }
    }

    warnings
}

async fn run_policy_test_case(
    policy_json: &serde_json::Value,
    test_case: &PolicyTestCase,
) -> PolicyTestResult {
    // Parse policy to extract effect
    let policy_effect = policy_json
        .get("effect")
        .and_then(|e| e.as_str())
        .unwrap_or("DENY");

    // Simple evaluation based on policy structure
    // Full ABAC engine integration required for complete policy evaluation
    let actual_decision = if policy_json.get("rules").and_then(|r| r.as_array()).is_some_and(|rules| !rules.is_empty()) {
        // If policy has rules, use the policy effect
        policy_effect.to_string()
    } else {
        "NOT_APPLICABLE".to_string()
    };

    let expected = test_case.expected_decision.clone();
    let passed = actual_decision == expected;

    PolicyTestResult {
        test_case_name: test_case.name.clone(),
        expected_decision: expected.clone(),
        actual_decision: actual_decision.clone(),
        passed,
        evaluation_details: Some(serde_json::json!({
            "policy_effect": policy_effect,
            "rules_count": policy_json.get("rules").and_then(|r| r.as_array()).map(|r| r.len()).unwrap_or(0),
            "note": "This is a simplified evaluation. Full ABAC engine integration required for complete testing."
        })),
        error: if passed {
            None
        } else {
            Some(format!(
                "Expected {} but got {}",
                expected, actual_decision
            ))
        },
    }
}
