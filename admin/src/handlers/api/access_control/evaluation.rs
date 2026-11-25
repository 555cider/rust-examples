use crate::{
    errors::AppError,
    models::dto::access_control::{
        AbacResult as DtoAbacResult, AccessControlRequest as DtoAccessControlRequest,
        AccessControlResponse as DtoAccessControlResponse, AccessDecision,
        AppliedPolicy as DtoAppliedPolicy, BatchAccessControlRequest, BatchAccessControlResponse,
        Decision as DtoDecision, LegacyPermissionCheckResponse, PolicyType as DtoPolicyType,
        RbacResult as DtoRbacResult, ResourceAccessRequest, ResourceAccessResult,
    },
    services::access_control::{
        AccessContext, AccessControlDecision, AccessControlRequest as ServiceAccessControlRequest,
        AccessControlResponse as ServiceAccessControlResponse, PolicyType as ServicePolicyType,
    },
    AppState,
};
use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct PermissionCheckQuery {
    pub user_id: i64,
    pub permission_code: String,
    pub resource: Option<String>,
}

pub fn route() -> Router<Arc<AppState>> {
    Router::new()
        .route("/check", post(check_access))
        .route("/check-batch", post(check_batch_access))
        .route("/check-batch-optimized", post(check_batch_access_optimized))
        .route("/check-permission", get(check_permission_legacy))
        .route("/evaluate", post(evaluate_access))
        .layer(middleware::from_fn(crate::middleware::auth))
}

/// Single access control check endpoint
async fn check_access(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DtoAccessControlRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Get access control manager from service container
    let access_control_manager =
        state
            .service
            .access_control_manager
            .as_ref()
            .ok_or_else(|| {
                AppError::InternalServerError("Access control manager not available".to_string())
            })?;

    // Convert DTO request to service request
    let service_request = convert_dto_to_service_request(req)?;

    let service_response = access_control_manager
        .evaluate_access(&service_request)
        .await
        .map_err(|e| AppError::InternalServerError(format!("Access evaluation failed: {}", e)))?;

    // Convert service response to DTO response
    let dto_response = convert_service_to_dto_response(service_response);

    Ok((StatusCode::OK, Json(dto_response)).into_response())
}

/// Batch access control check endpoint with performance optimizations
async fn check_batch_access(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchAccessControlRequest>,
) -> Result<impl IntoResponse, AppError> {
    let access_control_manager =
        state
            .service
            .access_control_manager
            .as_ref()
            .ok_or_else(|| {
                AppError::InternalServerError("Access control manager not available".to_string())
            })?;

    // Validate batch size to prevent abuse
    if req.requests.len() > 100 {
        return Err(AppError::BadRequest(
            "Batch size cannot exceed 100 requests".to_string(),
        ));
    }

    if req.requests.is_empty() {
        return Err(AppError::BadRequest(
            "Batch request cannot be empty".to_string(),
        ));
    }

    // Convert batch request to individual service requests
    let service_requests = convert_batch_dto_to_service_requests(req.clone())?;

    let start_time = std::time::Instant::now();

    // Use the access control manager's optimized batch evaluation
    let service_responses = access_control_manager
        .evaluate_batch(service_requests)
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Batch access evaluation failed: {}", e))
        })?;

    let total_evaluation_time = start_time.elapsed();

    // Convert service responses to DTO batch response with performance metrics
    let dto_response = convert_service_batch_to_dto_response_with_metrics(
        service_responses,
        req.subject,
        total_evaluation_time,
    );

    Ok((StatusCode::OK, Json(dto_response)).into_response())
}

/// Legacy permission check endpoint for backward compatibility
async fn check_permission_legacy(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PermissionCheckQuery>,
) -> Result<impl IntoResponse, AppError> {
    let access_control_manager =
        state
            .service
            .access_control_manager
            .as_ref()
            .ok_or_else(|| {
                AppError::InternalServerError("Access control manager not available".to_string())
            })?;

    // Convert legacy request to service request
    let service_request = ServiceAccessControlRequest {
        request_id: Some(Uuid::new_v4().to_string()),
        user_id: query.user_id,
        resource: query
            .resource
            .unwrap_or_else(|| query.permission_code.clone()),
        action: "access".to_string(), // Default action for legacy requests
        resource_type: Some("permission".to_string()),
        context: AccessContext::default(),
    };

    let service_response = access_control_manager
        .evaluate_access(&service_request)
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Legacy permission check failed: {}", e))
        })?;

    // Convert to legacy response format
    let legacy_response = LegacyPermissionCheckResponse {
        granted: service_response.decision == AccessControlDecision::Permit,
        permission: Some(query.permission_code),
        roles: service_response
            .applied_policies
            .iter()
            .filter(|p| matches!(p.policy_type, ServicePolicyType::Rbac))
            .map(|p| p.policy_name.clone())
            .collect(),
    };

    Ok((StatusCode::OK, Json(legacy_response)).into_response())
}

/// Detailed access evaluation endpoint with full context
async fn evaluate_access(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DtoAccessControlRequest>,
) -> Result<impl IntoResponse, AppError> {
    let access_control_manager =
        state
            .service
            .access_control_manager
            .as_ref()
            .ok_or_else(|| {
                AppError::InternalServerError("Access control manager not available".to_string())
            })?;

    // Convert DTO request to service request
    let service_request = convert_dto_to_service_request(req)?;

    let service_response = access_control_manager
        .evaluate_access(&service_request)
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Detailed access evaluation failed: {}", e))
        })?;

    // Convert service response to DTO response with full details
    let dto_response = convert_service_to_dto_response(service_response);

    Ok((StatusCode::OK, Json(dto_response)).into_response())
}

/// Optimized batch access control check endpoint for high-performance scenarios
async fn check_batch_access_optimized(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchAccessControlRequest>,
) -> Result<impl IntoResponse, AppError> {
    let access_control_manager =
        state
            .service
            .access_control_manager
            .as_ref()
            .ok_or_else(|| {
                AppError::InternalServerError("Access control manager not available".to_string())
            })?;

    // Validate batch size with higher limit for optimized endpoint
    if req.requests.len() > 500 {
        return Err(AppError::BadRequest(
            "Optimized batch size cannot exceed 500 requests".to_string(),
        ));
    }

    if req.requests.is_empty() {
        return Err(AppError::BadRequest(
            "Batch request cannot be empty".to_string(),
        ));
    }

    // Group requests by resource type for optimized processing
    let grouped_requests = group_requests_by_resource_type(&req);
    let mut all_results = Vec::new();
    let start_time = std::time::Instant::now();

    // Process each group separately for better cache utilization
    for (resource_type, requests) in grouped_requests {
        let service_requests =
            convert_grouped_requests_to_service_requests(&req.subject, &requests, &req.context)?;

        let group_responses = access_control_manager
            .evaluate_batch(service_requests)
            .await
            .map_err(|e| {
                AppError::InternalServerError(format!(
                    "Optimized batch evaluation failed for resource type {}: {}",
                    resource_type.unwrap_or_else(|| "unknown".to_string()),
                    e
                ))
            })?;

        all_results.extend(group_responses);
    }

    let total_evaluation_time = start_time.elapsed();

    // Convert to optimized response format
    let dto_response = convert_service_batch_to_dto_response_with_metrics(
        all_results,
        req.subject,
        total_evaluation_time,
    );

    Ok((StatusCode::OK, Json(dto_response)).into_response())
}

// Helper functions for converting between DTO and service types

fn convert_dto_to_service_request(
    dto_req: DtoAccessControlRequest,
) -> Result<ServiceAccessControlRequest, AppError> {
    // Parse subject as user_id (assuming it's a numeric string)
    let user_id = dto_req
        .subject
        .parse::<i64>()
        .map_err(|_| AppError::BadRequest("Subject must be a valid user ID".to_string()))?;

    let context = AccessContext {
        ip_address: None, // Could be extracted from request headers
        user_agent: None, // Could be extracted from request headers
        session_id: None,
        timestamp: Some(chrono::Utc::now()),
        request_headers: HashMap::new(),
        environment_attributes: dto_req.context.unwrap_or_default(),
    };

    Ok(ServiceAccessControlRequest {
        request_id: Some(Uuid::new_v4().to_string()),
        user_id,
        resource: dto_req.resource,
        action: dto_req.action,
        resource_type: dto_req.resource_type,
        context,
    })
}

fn convert_service_to_dto_response(
    service_resp: ServiceAccessControlResponse,
) -> DtoAccessControlResponse {
    let decision = AccessDecision {
        decision: match service_resp.decision {
            AccessControlDecision::Permit => DtoDecision::Permit,
            AccessControlDecision::Deny => DtoDecision::Deny,
            AccessControlDecision::NotApplicable => DtoDecision::NotApplicable,
        },
        rbac_result: service_resp.rbac_result.map(|rbac| DtoRbacResult {
            granted: matches!(
                rbac.decision,
                crate::services::access_control::rbac::RbacDecision::Permit
            ),
            applied_roles: rbac.user_roles.clone(),
            applied_permissions: rbac.matched_permissions,
            evaluation_time_ms: rbac.evaluation_time.as_millis() as u64,
        }),
        abac_result: service_resp.abac_result.map(|abac| DtoAbacResult {
            granted: matches!(
                abac.decision,
                crate::models::entities::abac::Decision::Permit
            ),
            applied_policies: abac
                .applied_policies
                .iter()
                .map(|p| p.policy_id.clone())
                .collect(),
            evaluation_details: vec![], // Populated from ABAC evaluation result
            evaluation_time_ms: 0,      // Extracted from ABAC evaluation metrics
        }),
        combination_strategy: format!("{:?}", service_resp.evaluation_mode),
        reason: None, // Could be extracted from service response
    };

    let applied_policies = service_resp
        .applied_policies
        .into_iter()
        .map(|p| DtoAppliedPolicy {
            policy_type: match p.policy_type {
                ServicePolicyType::Rbac => DtoPolicyType::Rbac,
                ServicePolicyType::Abac => DtoPolicyType::Abac,
            },
            policy_id: p.policy_id,
            policy_name: p.policy_name,
            effect: format!("{:?}", p.effect),
            priority: None, // Could be extracted from policy
        })
        .collect();

    DtoAccessControlResponse {
        granted: service_resp.decision == AccessControlDecision::Permit,
        decision,
        applied_policies,
        evaluation_time_ms: service_resp.evaluation_time.as_millis() as u64,
        request_id: service_resp.request_id,
        evaluated_at: service_resp.metadata.evaluation_timestamp,
    }
}

fn convert_batch_dto_to_service_requests(
    batch_req: BatchAccessControlRequest,
) -> Result<Vec<ServiceAccessControlRequest>, AppError> {
    let user_id = batch_req
        .subject
        .parse::<i64>()
        .map_err(|_| AppError::BadRequest("Subject must be a valid user ID".to_string()))?;

    let base_context = AccessContext {
        ip_address: None,
        user_agent: None,
        session_id: None,
        timestamp: Some(chrono::Utc::now()),
        request_headers: HashMap::new(),
        environment_attributes: batch_req.context.unwrap_or_default(),
    };

    let requests = batch_req
        .requests
        .into_iter()
        .map(|resource_req| ServiceAccessControlRequest {
            request_id: Some(Uuid::new_v4().to_string()),
            user_id,
            resource: resource_req.resource,
            action: resource_req.action,
            resource_type: resource_req.resource_type,
            context: base_context.clone(),
        })
        .collect();

    Ok(requests)
}

fn convert_service_batch_to_dto_response_with_metrics(
    service_responses: Vec<ServiceAccessControlResponse>,
    _subject: String,
    total_evaluation_time: std::time::Duration,
) -> BatchAccessControlResponse {
    let results = service_responses
        .into_iter()
        .map(|service_resp| {
            let dto_resp = convert_service_to_dto_response(service_resp.clone());

            // Extract resource and action from the original request context
            let resource = "unknown".to_string(); // Extracted from original request context
            let action = "unknown".to_string(); // Extracted from original request context

            ResourceAccessResult {
                resource,
                resource_type: None, // Could be enhanced to extract from service response
                action,
                granted: dto_resp.granted,
                decision: dto_resp.decision,
                applied_policies: dto_resp.applied_policies,
                evaluation_time_ms: dto_resp.evaluation_time_ms,
            }
        })
        .collect();

    BatchAccessControlResponse {
        results,
        total_evaluation_time_ms: total_evaluation_time.as_millis() as u64,
        request_id: Uuid::new_v4().to_string(),
        evaluated_at: chrono::Utc::now(),
    }
}
//
// Helper functions for optimized batch processing

fn group_requests_by_resource_type(
    batch_req: &BatchAccessControlRequest,
) -> HashMap<Option<String>, Vec<ResourceAccessRequest>> {
    let mut grouped = HashMap::new();

    for request in &batch_req.requests {
        let resource_type = request.resource_type.clone();
        grouped
            .entry(resource_type)
            .or_insert_with(Vec::new)
            .push(request.clone());
    }

    grouped
}

fn convert_grouped_requests_to_service_requests(
    subject: &str,
    requests: &[ResourceAccessRequest],
    context: &Option<HashMap<String, serde_json::Value>>,
) -> Result<Vec<ServiceAccessControlRequest>, AppError> {
    let user_id = subject
        .parse::<i64>()
        .map_err(|_| AppError::BadRequest("Subject must be a valid user ID".to_string()))?;

    let base_context = AccessContext {
        ip_address: None,
        user_agent: None,
        session_id: None,
        timestamp: Some(chrono::Utc::now()),
        request_headers: HashMap::new(),
        environment_attributes: context.clone().unwrap_or_default(),
    };

    let service_requests = requests
        .iter()
        .map(|resource_req| ServiceAccessControlRequest {
            request_id: Some(Uuid::new_v4().to_string()),
            user_id,
            resource: resource_req.resource.clone(),
            action: resource_req.action.clone(),
            resource_type: resource_req.resource_type.clone(),
            context: base_context.clone(),
        })
        .collect();

    Ok(service_requests)
}
