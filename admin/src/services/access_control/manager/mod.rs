use crate::{
    config::access_control::{AccessControlConfig, AccessControlConfigManager, AccessControlMode},
    errors::{
        access_control::{AccessControlError, AccessControlResult},
        context::ErrorContext,
    },
    models::entities::{
        abac::{AccessResponse, Decision},
        access_evaluation_log::AccessDecision as LogDecision,
    },
    services::{
        access_control::{
            abac::PolicyEngine,
            cache::{CacheInvalidationManager, CacheManager},
            hybrid::{HybridContext, HybridDecision, HybridEvaluator, HybridRequest, HybridResult},
            rbac::{RbacDecision, RbacEngine, RbacRequest, RbacResult},
        },
        access_evaluation_log::AccessEvaluationLogService,
        metrics::{MetricsCollector, PerformanceMetrics as ServiceMetrics},
    },
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Unified Access Control Manager
///
/// This is the main entry point for all access control operations.
/// It coordinates between RBAC, ABAC, and hybrid evaluation strategies.
#[derive(Clone)]
#[allow(dead_code)]
pub struct AccessControlManager {
    rbac_engine: Arc<RbacEngine>,
    abac_engine: Arc<PolicyEngine>,
    hybrid_evaluator: Arc<HybridEvaluator>,
    audit_service: Arc<AccessEvaluationLogService>,
    config_manager: Arc<AccessControlConfigManager>,
    cache_manager: Option<Arc<CacheManager>>,
    cache_invalidation_manager: Option<Arc<CacheInvalidationManager>>,
    metrics_collector: Option<Arc<MetricsCollector>>,
}

/// Unified access request
#[derive(Debug, Clone)]
pub struct AccessControlRequest {
    pub request_id: Option<String>,
    pub user_id: i64,
    pub resource: String,
    pub action: String,
    pub resource_type: Option<String>,
    pub context: AccessContext,
}

/// Access context with comprehensive information
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AccessContext {
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub request_headers: HashMap<String, String>,
    pub environment_attributes: HashMap<String, serde_json::Value>,
}

/// Unified access response
#[derive(Debug, Clone, Serialize)]
pub struct AccessControlResponse {
    pub request_id: String,
    pub decision: AccessControlDecision,
    pub evaluation_mode: AccessControlMode,
    pub rbac_result: Option<RbacResult>,
    pub abac_result: Option<AccessResponse>,
    pub hybrid_result: Option<HybridResult>,
    pub evaluation_time: Duration,
    pub applied_policies: Vec<AppliedPolicy>,
    pub audit_logged: bool,
    pub cache_hit: bool,
    pub metadata: ResponseMetadata,
}

/// Final access control decision
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum AccessControlDecision {
    Permit,
    Deny,
    NotApplicable,
}

/// Information about applied policies
#[derive(Debug, Clone, Serialize)]
pub struct AppliedPolicy {
    pub policy_type: PolicyType,
    pub policy_id: String,
    pub policy_name: String,
    pub effect: PolicyEffect,
    pub conditions_met: Vec<String>,
}

/// Type of policy that was applied
#[derive(Debug, Clone, Serialize)]
pub enum PolicyType {
    Rbac,
    Abac,
}

/// Effect of the policy
#[derive(Debug, Clone, Serialize)]
pub enum PolicyEffect {
    Permit,
    Deny,
}

/// Response metadata
#[derive(Debug, Clone, Serialize)]
pub struct ResponseMetadata {
    pub evaluation_timestamp: chrono::DateTime<chrono::Utc>,
    pub engine_versions: HashMap<String, String>,
    pub performance_metrics: PerformanceMetrics,
    pub warnings: Vec<String>,
}

/// Performance metrics for monitoring
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceMetrics {
    pub rbac_evaluation_time: Option<Duration>,
    pub abac_evaluation_time: Option<Duration>,
    pub total_evaluation_time: Duration,
    pub cache_lookups: u32,
    pub database_queries: u32,
}

impl AccessControlManager {
    pub fn new(
        rbac_engine: RbacEngine,
        abac_engine: PolicyEngine,
        hybrid_evaluator: HybridEvaluator,
        audit_service: AccessEvaluationLogService,
        config_manager: AccessControlConfigManager,
    ) -> Self {
        Self {
            rbac_engine: Arc::new(rbac_engine),
            abac_engine: Arc::new(abac_engine),
            hybrid_evaluator: Arc::new(hybrid_evaluator),
            audit_service: Arc::new(audit_service),
            config_manager: Arc::new(config_manager),
            cache_manager: None,
            cache_invalidation_manager: None,
            metrics_collector: None,
        }
    }

    /// Create access control manager with caching enabled
    pub fn with_caching(
        rbac_engine: RbacEngine,
        abac_engine: PolicyEngine,
        hybrid_evaluator: HybridEvaluator,
        audit_service: AccessEvaluationLogService,
        config_manager: AccessControlConfigManager,
        cache_manager: CacheManager,
        cache_invalidation_manager: CacheInvalidationManager,
    ) -> Self {
        Self {
            rbac_engine: Arc::new(rbac_engine),
            abac_engine: Arc::new(abac_engine),
            hybrid_evaluator: Arc::new(hybrid_evaluator),
            audit_service: Arc::new(audit_service),
            config_manager: Arc::new(config_manager),
            cache_manager: Some(Arc::new(cache_manager)),
            cache_invalidation_manager: Some(Arc::new(cache_invalidation_manager)),
            metrics_collector: None,
        }
    }

    /// Set metrics collector for performance monitoring
    pub fn with_metrics_collector(mut self, metrics_collector: Arc<MetricsCollector>) -> Self {
        self.metrics_collector = Some(metrics_collector);
        self
    }

    /// Main access control evaluation method
    pub async fn evaluate_access(
        &self,
        request: &AccessControlRequest,
    ) -> AccessControlResult<AccessControlResponse> {
        let start_time = Instant::now();
        let request_id = request
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        debug!(
            request_id = %request_id,
            user_id = request.user_id,
            resource = %request.resource,
            action = %request.action,
            "Starting access control evaluation"
        );

        let config = self.config_manager.get_config();
        let warnings = Vec::new();
        let mut cache_hit = false;

        // Check evaluation timeout
        let timeout_duration = Duration::from_millis(config.max_evaluation_time_ms);
        let evaluation_future = self.perform_evaluation(request, &config);

        let (decision, rbac_result, abac_result, hybrid_result, applied_policies) =
            match tokio::time::timeout(timeout_duration, evaluation_future).await {
                Ok(result) => result?,
                Err(_) => {
                    warn!(
                        request_id = %request_id,
                        user_id = request.user_id,
                        resource = %request.resource,
                        action = %request.action,
                        timeout_ms = timeout_duration.as_millis(),
                        "Access control evaluation timed out"
                    );
                    return Err(AccessControlError::EvaluationTimeout {
                        duration: timeout_duration,
                        context: ErrorContext::new("evaluate_access")
                            .with_user(request.user_id)
                            .with_resource(&request.resource)
                            .with_action(&request.action)
                            .with_data("request_id", request_id.to_string()),
                    });
                }
            };

        // Check for cache hits
        if let Some(ref rbac) = rbac_result {
            if rbac.cache_hit {
                cache_hit = true;
            }
        }

        let evaluation_time = start_time.elapsed();

        // Create response
        let response = AccessControlResponse {
            request_id: request_id.clone(),
            decision,
            evaluation_mode: config.mode,
            rbac_result,
            abac_result,
            hybrid_result,
            evaluation_time,
            applied_policies,
            audit_logged: false, // Will be updated after audit logging
            cache_hit,
            metadata: ResponseMetadata {
                evaluation_timestamp: chrono::Utc::now(),
                engine_versions: self.get_engine_versions(),
                performance_metrics: PerformanceMetrics {
                    rbac_evaluation_time: None,
                    abac_evaluation_time: None,
                    total_evaluation_time: evaluation_time,
                    cache_lookups: if cache_hit { 1 } else { 0 },
                    database_queries: 1, // Simplified for now
                },
                warnings,
            },
        };

        // Audit logging if enabled
        let mut final_response = response;
        if config.enable_audit_logging {
            match self
                .log_access_evaluation(&request_id, request, &final_response)
                .await
            {
                Ok(_) => {
                    final_response.audit_logged = true;
                    debug!(
                        request_id = %request_id,
                        "Access evaluation logged successfully"
                    );
                }
                Err(e) => {
                    warn!(
                        request_id = %request_id,
                        error = %e,
                        "Failed to log access evaluation"
                    );
                    final_response
                        .metadata
                        .warnings
                        .push(format!("Audit logging failed: {}", e));
                }
            }
        }

        info!(
            request_id = %request_id,
            user_id = request.user_id,
            resource = %request.resource,
            action = %request.action,
            decision = ?final_response.decision,
            evaluation_time_ms = evaluation_time.as_millis(),
            cache_hit = cache_hit,
            "Access control evaluation completed"
        );

        // Record metrics
        if let Some(collector) = &self.metrics_collector {
            let db_queries = final_response.metadata.performance_metrics.database_queries;
            let metric = ServiceMetrics::new("access_control_evaluate")
                .with_duration(evaluation_time.as_secs_f64() * 1000.0)
                .with_cache_hit(cache_hit)
                .with_db_queries(db_queries);
            collector.record(metric).await;

            // Periodically check cache hit rate (every 100 operations)
            if collector.count().await % 100 == 0 {
                collector.check_cache_hit_rate_warning().await;
            }
        }

        Ok(final_response)
    }

    /// Perform the actual evaluation based on configuration
    async fn perform_evaluation(
        &self,
        request: &AccessControlRequest,
        config: &AccessControlConfig,
    ) -> AccessControlResult<(
        AccessControlDecision,
        Option<RbacResult>,
        Option<AccessResponse>,
        Option<HybridResult>,
        Vec<AppliedPolicy>,
    )> {
        match config.mode {
            AccessControlMode::RbacOnly => {
                let rbac_request = self.convert_to_rbac_request(request);
                let rbac_result = self.rbac_engine.evaluate(&rbac_request).await?;
                let decision = self.convert_rbac_decision(&rbac_result.decision);
                let applied_policies = self.extract_rbac_policies(&rbac_result);

                Ok((decision, Some(rbac_result), None, None, applied_policies))
            }
            AccessControlMode::Hybrid => {
                let hybrid_request = self.convert_to_hybrid_request(request);
                let hybrid_result = self.hybrid_evaluator.evaluate(&hybrid_request).await?;
                let decision = self.convert_hybrid_decision(&hybrid_result.decision);
                let applied_policies = self.extract_hybrid_policies(&hybrid_result);

                Ok((
                    decision,
                    hybrid_result.rbac_result.clone(),
                    hybrid_result.abac_result.clone(),
                    Some(hybrid_result),
                    applied_policies,
                ))
            }
        }
    }

    /// Convert to RBAC request
    fn convert_to_rbac_request(&self, request: &AccessControlRequest) -> RbacRequest {
        RbacRequest {
            user_id: request.user_id,
            resource: request.resource.clone(),
            action: request.action.clone(),
            resource_type: request.resource_type.clone(),
        }
    }

    /// Convert to hybrid request
    fn convert_to_hybrid_request(&self, request: &AccessControlRequest) -> HybridRequest {
        HybridRequest {
            user_id: request.user_id,
            resource: request.resource.clone(),
            action: request.action.clone(),
            resource_type: request.resource_type.clone(),
            context: HybridContext {
                ip_address: request.context.ip_address.clone(),
                user_agent: request.context.user_agent.clone(),
                timestamp: request.context.timestamp,
                session_id: request.context.session_id.clone(),
                additional_attributes: request.context.environment_attributes.clone(),
            },
        }
    }

    /// Convert RBAC decision to access control decision
    fn convert_rbac_decision(&self, decision: &RbacDecision) -> AccessControlDecision {
        match decision {
            RbacDecision::Permit => AccessControlDecision::Permit,
            RbacDecision::Deny => AccessControlDecision::Deny,
            RbacDecision::NotApplicable => AccessControlDecision::NotApplicable,
        }
    }

    /// Convert hybrid decision to access control decision
    fn convert_hybrid_decision(&self, decision: &HybridDecision) -> AccessControlDecision {
        match decision {
            HybridDecision::Permit => AccessControlDecision::Permit,
            HybridDecision::Deny => AccessControlDecision::Deny,
            HybridDecision::NotApplicable => AccessControlDecision::NotApplicable,
        }
    }

    /// Extract applied policies from RBAC result
    fn extract_rbac_policies(&self, result: &RbacResult) -> Vec<AppliedPolicy> {
        result
            .matched_permissions
            .iter()
            .map(|perm| AppliedPolicy {
                policy_type: PolicyType::Rbac,
                policy_id: perm.clone(),
                policy_name: perm.clone(),
                effect: match result.decision {
                    RbacDecision::Permit => PolicyEffect::Permit,
                    _ => PolicyEffect::Deny,
                },
                conditions_met: vec![], // RBAC doesn't have complex conditions
            })
            .collect()
    }

    /// Extract applied policies from hybrid result
    fn extract_hybrid_policies(&self, result: &HybridResult) -> Vec<AppliedPolicy> {
        let mut policies = Vec::new();

        // Add RBAC policies
        if let Some(rbac_result) = &result.rbac_result {
            policies.extend(self.extract_rbac_policies(rbac_result));
        }

        // Add ABAC policies
        if let Some(abac_result) = &result.abac_result {
            for applied_policy in &abac_result.applied_policies {
                policies.push(AppliedPolicy {
                    policy_type: PolicyType::Abac,
                    policy_id: applied_policy.policy_id.clone(),
                    policy_name: applied_policy.policy_name.clone(),
                    effect: match applied_policy.effect {
                        crate::models::entities::abac::PolicyEffect::Permit => PolicyEffect::Permit,
                        crate::models::entities::abac::PolicyEffect::Deny => PolicyEffect::Deny,
                    },
                    conditions_met: applied_policy.matched_rules.clone(),
                });
            }
        }

        policies
    }

    /// Log access evaluation for audit purposes
    async fn log_access_evaluation(
        &self,
        request_id: &str,
        request: &AccessControlRequest,
        response: &AccessControlResponse,
    ) -> AccessControlResult<()> {
        let decision = match response.decision {
            AccessControlDecision::Permit => LogDecision::Permit,
            AccessControlDecision::Deny => LogDecision::Deny,
            AccessControlDecision::NotApplicable => LogDecision::NotApplicable,
        };

        let rbac_result = response.rbac_result.as_ref().map(|r| match r.decision {
            RbacDecision::Permit => LogDecision::Permit,
            RbacDecision::Deny => LogDecision::Deny,
            RbacDecision::NotApplicable => LogDecision::NotApplicable,
        });

        let abac_result = response.abac_result.as_ref().map(|r| match r.decision {
            Decision::Permit => LogDecision::Permit,
            Decision::Deny => LogDecision::Deny,
            Decision::NotApplicable => LogDecision::NotApplicable,
        });

        let applied_policies = response
            .applied_policies
            .iter()
            .map(|p| p.policy_id.clone())
            .collect::<Vec<_>>();

        let context = serde_json::to_value(&request.context).ok();

        let evaluation_time_ms = response.evaluation_time.as_millis() as i64;

        // Use the new method that accepts request_id for correlation
        self.audit_service
            .log_access_evaluation_with_request_id(
                request_id.to_string(),
                Some(request.user_id),
                Some(request.resource.clone()),
                request.resource_type.clone(),
                request.action.clone(),
                decision,
                rbac_result,
                abac_result,
                Some(applied_policies),
                Some(evaluation_time_ms),
                context,
            )
            .await
            .map_err(|e| AccessControlError::RbacError {
                message: format!("Failed to create audit log: {}", e),
                source: Some(Box::new(e)),
                context: ErrorContext::new("log_access_decision")
                    .with_data("request_id", request_id.to_string()),
            })?;

        Ok(())
    }

    /// Get engine versions for metadata
    fn get_engine_versions(&self) -> HashMap<String, String> {
        let mut versions = HashMap::new();
        versions.insert("rbac_engine".to_string(), "1.0.0".to_string());
        versions.insert("abac_engine".to_string(), "1.0.0".to_string());
        versions.insert("hybrid_evaluator".to_string(), "1.0.0".to_string());
        versions.insert("access_control_manager".to_string(), "1.0.0".to_string());
        versions
    }

    /// Simple permission check (legacy compatibility)
    pub async fn has_permission(
        &self,
        user_id: i64,
        resource: &str,
        action: &str,
    ) -> AccessControlResult<bool> {
        let request = AccessControlRequest {
            request_id: None,
            user_id,
            resource: resource.to_string(),
            action: action.to_string(),
            resource_type: None,
            context: AccessContext::default(),
        };

        let response = self.evaluate_access(&request).await?;
        Ok(response.decision == AccessControlDecision::Permit)
    }

    /// Batch permission evaluation
    pub async fn evaluate_batch(
        &self,
        requests: Vec<AccessControlRequest>,
    ) -> AccessControlResult<Vec<AccessControlResponse>> {
        let mut responses = Vec::new();

        // Process requests in parallel for better performance
        let futures: Vec<_> = requests
            .iter()
            .map(|req| self.evaluate_access(req))
            .collect();

        for future in futures {
            match future.await {
                Ok(response) => responses.push(response),
                Err(e) => {
                    error!("Batch evaluation failed for one request: {}", e);
                    // Continue with other requests, but log the error
                }
            }
        }

        Ok(responses)
    }

    /// Update access control configuration at runtime
    pub async fn update_config(&self, new_config: AccessControlConfig) -> AccessControlResult<()> {
        // Update the configuration
        let ctx = ErrorContext::new("update_configuration");

        self.config_manager.set_mode(new_config.mode).map_err(|e| {
            AccessControlError::ConfigurationError {
                message: e,
                context: ctx.clone(),
            }
        })?;
        self.config_manager
            .set_combination_strategy(new_config.combination_strategy)
            .map_err(|e| AccessControlError::ConfigurationError {
                message: e,
                context: ctx.clone(),
            })?;
        self.config_manager
            .set_conflict_resolution(new_config.conflict_resolution)
            .map_err(|e| AccessControlError::ConfigurationError {
                message: e,
                context: ctx.clone(),
            })?;
        self.config_manager
            .set_caching_enabled(new_config.enable_caching)
            .map_err(|e| AccessControlError::ConfigurationError {
                message: e,
                context: ctx.clone(),
            })?;
        self.config_manager
            .set_cache_ttl(new_config.cache_ttl_seconds)
            .map_err(|e| AccessControlError::ConfigurationError {
                message: e,
                context: ctx.clone(),
            })?;
        self.config_manager
            .set_max_evaluation_time(new_config.max_evaluation_time_ms)
            .map_err(|e| AccessControlError::ConfigurationError {
                message: e,
                context: ctx.clone(),
            })?;
        self.config_manager
            .set_audit_logging_enabled(new_config.enable_audit_logging)
            .map_err(|e| AccessControlError::ConfigurationError {
                message: e,
                context: ctx.clone(),
            })?;

        info!("Access control configuration updated successfully");
        Ok(())
    }

    /// Get current configuration
    pub fn get_config(&self) -> AccessControlConfig {
        self.config_manager.get_config_clone()
    }

    /// Invalidate cache for a specific user
    pub async fn invalidate_user_cache(&self, user_id: i64) {
        if let Some(invalidation_manager) = &self.cache_invalidation_manager {
            if let Err(e) = invalidation_manager
                .on_user_permissions_changed(user_id)
                .await
            {
                warn!(
                    "Failed to invalidate user cache through invalidation manager: {}",
                    e
                );
            }
        } else {
            // Fallback to direct cache invalidation
            self.rbac_engine.invalidate_user_cache(user_id).await;
            self.hybrid_evaluator.invalidate_user_cache(user_id).await;
        }
        info!("Invalidated access control cache for user {}", user_id);
    }

    /// Clear all caches
    pub async fn clear_all_caches(&self) {
        if let Some(invalidation_manager) = &self.cache_invalidation_manager {
            if let Err(e) = invalidation_manager.clear_all().await {
                warn!(
                    "Failed to clear all caches through invalidation manager: {}",
                    e
                );
            }
        } else {
            // Fallback to direct cache clearing
            self.rbac_engine.clear_cache().await;
            self.hybrid_evaluator.clear_cache().await;
        }
        info!("Cleared all access control caches");
    }

    /// Invalidate cache when policy changes
    pub async fn invalidate_policy_cache(&self, policy_id: &str) -> AccessControlResult<()> {
        if let Some(invalidation_manager) = &self.cache_invalidation_manager {
            invalidation_manager
                .on_policy_updated(policy_id.to_string())
                .await?;
        }
        info!("Invalidated policy cache for policy {}", policy_id);
        Ok(())
    }

    /// Invalidate cache when resource attributes change
    pub async fn invalidate_resource_cache(
        &self,
        resource_id: &str,
        resource_type: &str,
    ) -> AccessControlResult<()> {
        if let Some(invalidation_manager) = &self.cache_invalidation_manager {
            invalidation_manager
                .on_resource_attributes_changed(resource_id.to_string(), resource_type.to_string())
                .await?;
        }
        info!(
            "Invalidated resource cache for resource {} of type {}",
            resource_id, resource_type
        );
        Ok(())
    }

    /// Get comprehensive statistics
    pub async fn get_statistics(&self) -> AccessControlManagerStats {
        let rbac_stats = self.rbac_engine.get_cache_stats().await;
        let hybrid_stats = self.hybrid_evaluator.get_statistics().await;
        let config = self.config_manager.get_config();

        AccessControlManagerStats {
            current_mode: config.mode,
            rbac_cache_stats: rbac_stats,
            hybrid_evaluator_stats: hybrid_stats,
            configuration: (*config).clone(),
            uptime: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default(),
        }
    }

    /// Get cache manager statistics (if available)
    pub async fn get_cache_statistics(
        &self,
    ) -> Option<crate::services::access_control::cache::CacheManagerStats> {
        if let Some(cache_manager) = &self.cache_manager {
            Some(cache_manager.get_stats().await)
        } else {
            None
        }
    }

    /// Get cache invalidation statistics (if available)
    pub async fn get_invalidation_statistics(
        &self,
    ) -> Option<crate::services::access_control::cache::InvalidationStats> {
        if let Some(invalidation_manager) = &self.cache_invalidation_manager {
            Some(invalidation_manager.get_stats().await)
        } else {
            None
        }
    }
}

/// Access Control Manager statistics
#[derive(Debug, Clone, Serialize)]
pub struct AccessControlManagerStats {
    pub current_mode: AccessControlMode,
    pub rbac_cache_stats: crate::services::access_control::rbac::RbacCacheStats,
    pub hybrid_evaluator_stats: crate::services::access_control::hybrid::HybridEvaluatorStats,
    pub configuration: AccessControlConfig,
    pub uptime: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_access_control_decision_conversion() {
        let manager = create_test_manager().await;

        assert_eq!(
            manager.convert_rbac_decision(&RbacDecision::Permit),
            AccessControlDecision::Permit
        );
        assert_eq!(
            manager.convert_rbac_decision(&RbacDecision::Deny),
            AccessControlDecision::Deny
        );
        assert_eq!(
            manager.convert_rbac_decision(&RbacDecision::NotApplicable),
            AccessControlDecision::NotApplicable
        );
    }

    #[tokio::test]
    async fn test_engine_versions() {
        let manager = create_test_manager().await;
        let versions = manager.get_engine_versions();

        assert!(versions.contains_key("rbac_engine"));
        assert!(versions.contains_key("abac_engine"));
        assert!(versions.contains_key("hybrid_evaluator"));
        assert!(versions.contains_key("access_control_manager"));
    }

    async fn create_test_manager() -> AccessControlManager {
        use crate::config::access_control::{AccessControlConfig, AccessControlConfigManager};
        use crate::repositories::{
            policy::PolicyRepository, PermissionRepository, RoleRepository, UserRepository,
        };
        use crate::services::access_evaluation_log::AccessEvaluationLogService;
        use sqlx::sqlite::SqlitePoolOptions;

        // Create in-memory pool
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory DB for mock");

        let pool = std::sync::Arc::new(pool);

        let config = AccessControlConfig::from_env();

        let rbac_engine = RbacEngine::new(
            PermissionRepository::new(pool.clone()),
            UserRepository::new(pool.clone()),
            RoleRepository::new(pool.clone()),
            config.clone(),
        );

        let policy_engine = PolicyEngine::new(PolicyRepository::new(pool.clone()));

        let hybrid_evaluator =
            HybridEvaluator::new(rbac_engine.clone(), policy_engine.clone(), config.clone());

        use crate::repositories::access_evaluation_log::AccessEvaluationLogRepository;

        let audit_repo = AccessEvaluationLogRepository::new(pool.clone());
        let audit_service = AccessEvaluationLogService::new(audit_repo);
        let config_manager = AccessControlConfigManager::new(config);

        AccessControlManager::new(
            rbac_engine,
            policy_engine,
            hybrid_evaluator,
            audit_service,
            config_manager,
        )
    }
}
