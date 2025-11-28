use crate::{
    errors::AppError,
    models::entities::abac::{
        AccessRequest, AccessResponse, AppliedPolicy, AttributeValue, Condition,
        ConditionExpression, Decision, Operator, Policy, PolicyEffect, Rule, Target,
    },
    repositories::policy::PolicyRepository,
    services::{
        access_control::cache::CacheManager,
        metrics::{MetricsCollector, PerformanceMetrics},
    },
};
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

mod condition;
mod evaluator;
mod operator;
mod expression_evaluator;

pub use condition::*;
pub use operator::*;
pub use expression_evaluator::ExpressionEvaluator;

/// Policy evaluation context containing all attributes for evaluation
#[derive(Debug, Clone)]
pub struct PolicyEvaluationContext {
    pub subject_attributes: HashMap<String, AttributeValue>,
    pub resource_attributes: HashMap<String, AttributeValue>,
    pub environment_attributes: HashMap<String, AttributeValue>,
    pub action: String,
}

/// Result of policy evaluation
#[derive(Debug, Clone)]
pub struct PolicyEvaluationResult {
    pub decision: Decision,
    pub applicable_policies: Vec<String>,
    pub evaluation_details: Vec<EvaluationDetail>,
}

/// Details about individual policy evaluation
#[derive(Debug, Clone)]
pub struct EvaluationDetail {
    pub policy_id: String,
    pub policy_name: String,
    pub matched: bool,
    pub effect: PolicyEffect,
    pub rule_results: Vec<RuleEvaluationResult>,
}

/// Result of individual rule evaluation
#[derive(Debug, Clone)]
pub struct RuleEvaluationResult {
    pub rule_index: usize,
    pub condition_result: bool,
    pub effect: PolicyEffect,
    pub error: Option<String>,
}

/// ABAC Policy Engine for evaluating attribute-based access control policies
#[derive(Clone)]
pub struct PolicyEngine {
    policy_repository: PolicyRepository,
    rule_evaluator: RuleEvaluator,
    cache_manager: Option<std::sync::Arc<CacheManager>>,
    metrics_collector: Option<Arc<MetricsCollector>>,
}

impl PolicyEngine {
    /// Create a new policy engine
    pub fn new(policy_repository: PolicyRepository) -> Self {
        Self {
            policy_repository,
            rule_evaluator: RuleEvaluator::new(),
            cache_manager: None,
            metrics_collector: None,
        }
    }

    /// Create a new policy engine with cache manager
    pub fn with_cache_manager(
        policy_repository: PolicyRepository,
        cache_manager: CacheManager,
    ) -> Self {
        Self {
            policy_repository,
            rule_evaluator: RuleEvaluator::new(),
            cache_manager: Some(std::sync::Arc::new(cache_manager)),
            metrics_collector: None,
        }
    }

    /// Set metrics collector for performance monitoring
    pub fn with_metrics_collector(mut self, metrics_collector: Arc<MetricsCollector>) -> Self {
        self.metrics_collector = Some(metrics_collector);
        self
    }

    /// Build evaluation context from access request
    pub(crate) fn build_evaluation_context(
        &self,
        request: &AccessRequest,
    ) -> PolicyEvaluationContext {
        PolicyEvaluationContext {
            subject_attributes: request.subject.attributes.clone(),
            resource_attributes: request.resource.attributes.clone(),
            environment_attributes: request.context.environment.clone(),
            action: request.action.name.clone(),
        }
    }

    /// Simple pattern matching (supports wildcards)
    pub(crate) fn match_pattern(&self, pattern: &str, value: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        if pattern.contains('*') {
            let regex_pattern = pattern.replace('*', ".*");
            if let Ok(regex) = Regex::new(&format!("^{}$", regex_pattern)) {
                return regex.is_match(value);
            }
        }

        pattern == value
    }

    /// Get active policies with caching
    pub(crate) async fn get_active_policies_cached(&self) -> Result<Vec<Policy>, AppError> {
        // For now, just get from repository
        // In a more sophisticated implementation, we could cache the entire policy list
        self.policy_repository.find_active_policies().await
    }

    /// Get policy by ID for testing/debugging
    pub async fn get_policy(&self, policy_id: &str) -> Result<Policy, AppError> {
        // Try cache first if available
        if let Some(cache_manager) = &self.cache_manager {
            if let Some(cached_policy) = cache_manager.get_policy(policy_id).await {
                return Ok(cached_policy);
            }
        }

        // Get from repository and cache if available
        let policy = self.policy_repository.find_by_id(policy_id).await?;

        if let Some(cache_manager) = &self.cache_manager {
            cache_manager.cache_policy(&policy).await;
        }

        Ok(policy)
    }

    /// Test policy evaluation with given context
    pub async fn test_policy(
        &self,
        policy_id: &str,
        context: &PolicyEvaluationContext,
    ) -> Result<EvaluationDetail, AppError> {
        let policy = self.policy_repository.find_by_id(policy_id).await?;
        self.evaluate_policy(&policy, context).await
    }
}

/// Rule evaluator for processing individual policy rules
#[derive(Clone)]
pub struct RuleEvaluator {
    expression_evaluator: ExpressionEvaluator,
}

impl RuleEvaluator {
    pub fn new() -> Self {
        Self {
            expression_evaluator: ExpressionEvaluator::new(),
        }
    }

    /// Evaluate a single rule
    pub fn evaluate_rule(
        &self,
        rule: &Rule,
        context: &PolicyEvaluationContext,
    ) -> Result<RuleEvaluationResult, AppError> {
        let all_attributes = self.merge_attributes(context);
        
        // Convert AttributeValue map to JsonValue map
        let context_json_map: HashMap<String, serde_json::Value> = all_attributes
            .into_iter()
            .map(|(k, v)| (k, v.into()))
            .collect();

        match self.expression_evaluator.evaluate_condition(&rule.condition.expression, &context_json_map) {
            Ok(result) => Ok(RuleEvaluationResult {
                rule_index: 0, // Will be set by caller
                condition_result: result,
                effect: rule.effect,
                error: None,
            }),
            Err(e) => Ok(RuleEvaluationResult {
                rule_index: 0,
                condition_result: false,
                effect: rule.effect,
                error: Some(e.to_string()),
            }),
        }
    }

    /// Merge all attribute contexts into a single map
    fn merge_attributes(
        &self,
        context: &PolicyEvaluationContext,
    ) -> HashMap<String, AttributeValue> {
        let mut all_attributes = HashMap::new();
        
        for (key, value) in &context.subject_attributes {
            all_attributes.insert(format!("subject.{}", key), value.clone());
        }
        for (key, value) in &context.resource_attributes {
            all_attributes.insert(format!("resource.{}", key), value.clone());
        }
        for (key, value) in &context.environment_attributes {
            all_attributes.insert(format!("environment.{}", key), value.clone());
        }
        all_attributes.insert("action".to_string(), AttributeValue::String(context.action.clone()));
        
        all_attributes
    }
}
