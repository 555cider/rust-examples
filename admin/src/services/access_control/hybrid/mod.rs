use crate::{
    config::access_control::AccessControlConfig,
    errors::access_control::{AccessControlError, AccessControlResult},
    models::entities::abac::{AccessRequest, AccessResponse, Decision, PolicyEffect},
    services::access_control::{
        abac::PolicyEngine,
        rbac::{RbacDecision, RbacEngine, RbacRequest, RbacResult},
    },
};
use serde::Serialize;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{debug, info, warn};

/// Hybrid evaluator that combines RBAC and ABAC decisions
#[derive(Clone)]
pub struct HybridEvaluator {
    rbac_engine: Arc<RbacEngine>,
    abac_engine: Arc<PolicyEngine>,
    config: AccessControlConfig,
}

/// Hybrid evaluation request
#[derive(Debug, Clone)]
pub struct HybridRequest {
    pub user_id: i64,
    pub resource: String,
    pub action: String,
    pub resource_type: Option<String>,
    pub context: HybridContext,
}

/// Context information for hybrid evaluation
#[derive(Debug, Clone, Default)]
pub struct HybridContext {
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub session_id: Option<String>,
    pub additional_attributes: std::collections::HashMap<String, serde_json::Value>,
}

/// Hybrid evaluation result
#[derive(Debug, Clone, Serialize)]
pub struct HybridResult {
    pub decision: HybridDecision,
    pub rbac_result: Option<RbacResult>,
    pub abac_result: Option<AccessResponse>,
    pub evaluation_time: Duration,
    pub evaluation_details: EvaluationDetails,
}

/// Final hybrid decision
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HybridDecision {
    Permit,
    Deny,
    NotApplicable,
}

/// Detailed evaluation information
#[derive(Debug, Clone, Serialize)]
pub struct EvaluationDetails {
    pub rbac_evaluated: bool,
    pub abac_evaluated: bool,
    pub rbac_decision: Option<RbacDecision>,
    pub abac_decision: Option<Decision>,
    pub reason: String,
}

/// Statistics for the hybrid evaluator
#[derive(Debug, Clone, Serialize, Default)]
pub struct HybridEvaluatorStats {
    pub evaluations_count: u64,
    pub cache_hits: u64,
    pub average_evaluation_time_ms: f64,
}

impl HybridEvaluator {
    pub fn new(
        rbac_engine: RbacEngine,
        abac_engine: PolicyEngine,
        config: AccessControlConfig,
    ) -> Self {
        Self {
            rbac_engine: Arc::new(rbac_engine),
            abac_engine: Arc::new(abac_engine),
            config,
        }
    }

    /// Evaluate access request using hybrid approach
    /// Logic:
    /// 1. Check ABAC Deny Policies (Safety Net)
    /// 2. Check RBAC (Baseline Access)
    /// 3. Check ABAC Permit Policies (Exception/Contextual Access)
    pub async fn evaluate(&self, request: &HybridRequest) -> AccessControlResult<HybridResult> {
        let start_time = Instant::now();

        debug!(
            "Starting hybrid evaluation for user {} on resource {} with action {}",
            request.user_id, request.resource, request.action
        );

        let mut details = EvaluationDetails {
            rbac_evaluated: false,
            abac_evaluated: false,
            rbac_decision: None,
            abac_decision: None,
            reason: String::new(),
        };

        // 1. Evaluate ABAC first to check for explicit DENY
        let abac_result = self.evaluate_abac(request, &mut details).await?;
        
        // Check for explicit DENY in ABAC policies
        let mut explicit_deny = false;
        let mut explicit_permit = false;

        if let Some(response) = &abac_result {
            if response.decision == Decision::Deny {
                explicit_deny = true;
            } else if response.decision == Decision::Permit {
                explicit_permit = true;
            }
        }

        if explicit_deny {
            details.reason = "Access denied by ABAC policy".to_string();
            return Ok(HybridResult {
                decision: HybridDecision::Deny,
                rbac_result: None,
                abac_result,
                evaluation_time: start_time.elapsed(),
                evaluation_details: details,
            });
        }

        // 2. Evaluate RBAC
        let rbac_result = self.evaluate_rbac(request, &mut details).await?;
        
        if rbac_result.decision == RbacDecision::Permit {
            details.reason = "Access granted by RBAC".to_string();
            return Ok(HybridResult {
                decision: HybridDecision::Permit,
                rbac_result: Some(rbac_result),
                abac_result,
                evaluation_time: start_time.elapsed(),
                evaluation_details: details,
            });
        }

        // 3. If RBAC denied, check if ABAC explicitly permitted
        if explicit_permit {
            details.reason = "Access granted by ABAC policy (RBAC denied)".to_string();
            return Ok(HybridResult {
                decision: HybridDecision::Permit,
                rbac_result: Some(rbac_result),
                abac_result,
                evaluation_time: start_time.elapsed(),
                evaluation_details: details,
            });
        }

        // Default Deny
        details.reason = "Access denied (RBAC denied and no matching ABAC permit policy)".to_string();
        Ok(HybridResult {
            decision: HybridDecision::Deny,
            rbac_result: Some(rbac_result),
            abac_result,
            evaluation_time: start_time.elapsed(),
            evaluation_details: details,
        })
    }

    /// Evaluate RBAC engine
    async fn evaluate_rbac(
        &self,
        request: &HybridRequest,
        details: &mut EvaluationDetails,
    ) -> AccessControlResult<RbacResult> {
        let rbac_request = RbacRequest {
            user_id: request.user_id,
            resource: request.resource.clone(),
            action: request.action.clone(),
            resource_type: request.resource_type.clone(),
        };

        match self.rbac_engine.evaluate(&rbac_request).await {
            Ok(result) => {
                details.rbac_evaluated = true;
                details.rbac_decision = Some(result.decision.clone());
                Ok(result)
            }
            Err(e) => {
                warn!("RBAC evaluation failed: {}", e);
                Err(e)
            }
        }
    }

    /// Evaluate ABAC engine
    async fn evaluate_abac(
        &self,
        request: &HybridRequest,
        details: &mut EvaluationDetails,
    ) -> AccessControlResult<Option<AccessResponse>> {
        if !self.config.is_abac_enabled() {
            return Ok(None);
        }

        // Fetch user attributes
        let mut user_attributes = std::collections::HashMap::new();
        if let Ok(user_with_roles) = self.rbac_engine.get_user_with_roles(request.user_id).await {
            if let Some(attrs_str) = user_with_roles.user.attributes {
                if let Ok(json_map) = serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(&attrs_str) {
                    for (k, v) in json_map {
                        let attr_value = match v {
                            serde_json::Value::String(s) => crate::models::entities::abac::AttributeValue::String(s),
                            serde_json::Value::Number(n) => {
                                if let Some(f) = n.as_f64() {
                                    crate::models::entities::abac::AttributeValue::Number(f)
                                } else {
                                    crate::models::entities::abac::AttributeValue::String(n.to_string())
                                }
                            }
                            serde_json::Value::Bool(b) => crate::models::entities::abac::AttributeValue::Boolean(b),
                            serde_json::Value::Array(arr) => {
                                let converted_arr: Vec<crate::models::entities::abac::AttributeValue> = arr
                                    .iter()
                                    .map(|v| match v {
                                        serde_json::Value::String(s) => crate::models::entities::abac::AttributeValue::String(s.clone()),
                                        serde_json::Value::Number(n) => {
                                            crate::models::entities::abac::AttributeValue::Number(n.as_f64().unwrap_or(0.0))
                                        }
                                        serde_json::Value::Bool(b) => crate::models::entities::abac::AttributeValue::Boolean(*b),
                                        _ => crate::models::entities::abac::AttributeValue::String(v.to_string()),
                                    })
                                    .collect();
                                crate::models::entities::abac::AttributeValue::Array(converted_arr)
                            }
                            serde_json::Value::Null => crate::models::entities::abac::AttributeValue::Null,
                            _ => crate::models::entities::abac::AttributeValue::String(v.to_string()),
                        };
                        user_attributes.insert(k, attr_value);
                    }
                } else {
                    warn!("Failed to parse user attributes JSON: {}", attrs_str);
                }
            }
        } else {
            warn!("Failed to get user with roles for user_id: {}", request.user_id);
        }

        debug!("Populated user attributes for ABAC: {:?}", user_attributes);

        let abac_request = AccessRequest {
            subject: crate::models::entities::abac::Subject {
                id: request.user_id.to_string(),
                attributes: user_attributes,
            },
            resource: crate::models::entities::abac::Resource {
                id: request.resource.clone(),
                resource_type: request.resource_type.clone().unwrap_or_default(),
                attributes: std::collections::HashMap::new(), // Resource attributes populated from resource service
            },
            action: crate::models::entities::abac::Action {
                name: request.action.clone(),
                attributes: None,
            },
            context: self.convert_context(&request.context),
        };

        match self.abac_engine.evaluate_access(&abac_request).await {
            Ok(result) => {
                details.abac_evaluated = true;
                details.abac_decision = Some(result.decision.clone());

                // Convert PolicyEvaluationResult to AccessResponse
                let access_response = AccessResponse {
                    decision: result.decision,
                    applied_policies: result
                        .applicable_policies
                        .into_iter()
                        .map(|policy_id| {
                            crate::models::entities::abac::AppliedPolicy {
                                policy_id: policy_id.clone(),
                                policy_name: policy_id, 
                                effect: match result.decision {
                                    Decision::Permit => PolicyEffect::Permit,
                                    _ => PolicyEffect::Deny,
                                },
                                matched_rules: vec![], 
                            }
                        })
                        .collect(),
                    evaluation_time: std::time::Duration::from_millis(10), // Placeholder
                    audit_info: crate::models::entities::abac::AuditInfo {
                        request_id: "hybrid-eval".to_string(),
                        evaluation_method: crate::models::entities::abac::EvaluationMethod::Hybrid,
                        rbac_result: None,
                        abac_result: Some(result.decision.clone()),
                        final_decision: result.decision.clone(),
                        decision_reason: "ABAC evaluation completed".to_string(),
                    },
                };

                Ok(Some(access_response))
            }
            Err(e) => {
                warn!("ABAC evaluation failed: {}", e);
                Err(AccessControlError::AbacError {
                    message: e.to_string(),
                    source: None,
                    context: crate::errors::context::ErrorContext::new("abac_evaluation"),
                })
            }
        }
    }

    /// Convert hybrid context to ABAC context
    fn convert_context(&self, context: &HybridContext) -> crate::models::entities::abac::Context {
        use crate::models::entities::abac::{AttributeValue, Context};

        let mut environment = std::collections::HashMap::new();

        // Convert additional attributes
        for (key, value) in &context.additional_attributes {
            let attr_value = match value {
                serde_json::Value::String(s) => AttributeValue::String(s.clone()),
                serde_json::Value::Number(n) => {
                    if let Some(f) = n.as_f64() {
                        AttributeValue::Number(f)
                    } else {
                        AttributeValue::String(n.to_string())
                    }
                }
                serde_json::Value::Bool(b) => AttributeValue::Boolean(*b),
                serde_json::Value::Array(arr) => {
                    let converted_arr: Vec<AttributeValue> = arr
                        .iter()
                        .map(|v| match v {
                            serde_json::Value::String(s) => AttributeValue::String(s.clone()),
                            serde_json::Value::Number(n) => {
                                AttributeValue::Number(n.as_f64().unwrap_or(0.0))
                            }
                            serde_json::Value::Bool(b) => AttributeValue::Boolean(*b),
                            _ => AttributeValue::String(v.to_string()),
                        })
                        .collect();
                    AttributeValue::Array(converted_arr)
                }
                serde_json::Value::Null => AttributeValue::Null,
                _ => AttributeValue::String(value.to_string()),
            };
            environment.insert(key.clone(), attr_value);
        }

        if let Some(ip) = &context.ip_address {
            environment.insert("ip_address".to_string(), AttributeValue::String(ip.clone()));
        }

        if let Some(ua) = &context.user_agent {
            environment.insert("user_agent".to_string(), AttributeValue::String(ua.clone()));
        }

        if let Some(session) = &context.session_id {
            environment.insert(
                "session_id".to_string(),
                AttributeValue::String(session.clone()),
            );
        }

        Context {
            environment,
            request_time: context.timestamp.unwrap_or_else(chrono::Utc::now),
            session_id: context.session_id.clone(),
            ip_address: context.ip_address.clone(),
            user_agent: context.user_agent.clone(),
        }
    }

    /// Check if user has permission (legacy compatibility)
    pub async fn has_permission(
        &self,
        user_id: i64,
        resource: &str,
        action: &str,
    ) -> AccessControlResult<bool> {
        let request = HybridRequest {
            user_id,
            resource: resource.to_string(),
            action: action.to_string(),
            resource_type: None,
            context: HybridContext::default(),
        };

        let result = self.evaluate(&request).await?;
        Ok(result.decision == HybridDecision::Permit)
    }

    /// Invalidate cache for a specific user
    pub async fn invalidate_user_cache(&self, user_id: i64) {
        self.rbac_engine.invalidate_user_cache(user_id).await;
        info!("Invalidated hybrid evaluator cache for user {}", user_id);
    }

    /// Clear all caches
    pub async fn clear_cache(&self) {
        self.rbac_engine.clear_cache().await;
        info!("Cleared all hybrid evaluator caches");
    }

    /// Get statistics
    pub async fn get_statistics(&self) -> HybridEvaluatorStats {
        HybridEvaluatorStats::default()
    }
}
