use super::*;

impl PolicyEngine {
    /// Evaluate access request against all applicable policies
    pub async fn evaluate_access(
        &self,
        request: &AccessRequest,
    ) -> Result<PolicyEvaluationResult, AppError> {
        let start_time = Instant::now();
        let cache_hit = false;
        let mut db_queries = 0u32;

        // Check cache first if available
        if let Some(cache_manager) = &self.cache_manager {
            if let Some(cached_response) = cache_manager.get_abac_result(request).await {
                // Record metrics for cache hit
                if let Some(collector) = &self.metrics_collector {
                    let metric = PerformanceMetrics::new("abac_evaluate_access")
                        .with_duration(start_time.elapsed().as_secs_f64() * 1000.0)
                        .with_cache_hit(true)
                        .with_db_queries(0);
                    collector.record(metric).await;
                }

                // Convert cached response to evaluation result
                return Ok(PolicyEvaluationResult {
                    decision: cached_response.decision,
                    applicable_policies: cached_response
                        .applied_policies
                        .iter()
                        .map(|p| p.policy_id.clone())
                        .collect(),
                    evaluation_details: vec![], // Simplified for cached results
                });
            }
        }

        // Build evaluation context from the access request
        let context = self.build_evaluation_context(request);

        // Get all active policies (with caching)
        let policies = self.get_active_policies_cached().await?;
        db_queries += 1; // Policy fetch

        let mut evaluation_details = Vec::new();
        let mut applicable_policies = Vec::new();
        let mut final_decision = Decision::NotApplicable;

        // Evaluate each policy
        for policy in policies {
            let detail = self.evaluate_policy(&policy, &context).await?;

            if detail.matched {
                applicable_policies.push(policy.id.clone());

                // Apply policy combining algorithm (first applicable wins for now)
                if final_decision == Decision::NotApplicable {
                    final_decision = match detail.effect {
                        PolicyEffect::Permit => Decision::Permit,
                        PolicyEffect::Deny => Decision::Deny,
                    };
                }
            }

            evaluation_details.push(detail);
        }

        let result = PolicyEvaluationResult {
            decision: final_decision,
            applicable_policies,
            evaluation_details,
        };

        // Cache the result if cache manager is available
        if let Some(cache_manager) = &self.cache_manager {
            let access_response = AccessResponse {
                decision: result.decision.clone(),
                applied_policies: result
                    .applicable_policies
                    .iter()
                    .map(|policy_id| {
                        AppliedPolicy {
                            policy_id: policy_id.clone(),
                            policy_name: policy_id.clone(),
                            effect: match result.decision {
                                Decision::Permit => {
                                    crate::models::entities::abac::PolicyEffect::Permit
                                }
                                _ => crate::models::entities::abac::PolicyEffect::Deny,
                            },
                            matched_rules: vec![],
                        }
                    })
                    .collect(),
                evaluation_time: std::time::Duration::from_millis(10), // Placeholder
                audit_info: crate::models::entities::abac::AuditInfo {
                    request_id: "policy-eval".to_string(),
                    evaluation_method: crate::models::entities::abac::EvaluationMethod::Hybrid,
                    rbac_result: None,
                    abac_result: Some(result.decision.clone()),
                    final_decision: result.decision.clone(),
                    decision_reason: "ABAC policy evaluation completed".to_string(),
                },
            };
            cache_manager
                .cache_abac_result(request, &access_response)
                .await;
        }

        // Record metrics
        if let Some(collector) = &self.metrics_collector {
            let metric = PerformanceMetrics::new("abac_evaluate_access")
                .with_duration(start_time.elapsed().as_secs_f64() * 1000.0)
                .with_cache_hit(cache_hit)
                .with_db_queries(db_queries);
            collector.record(metric).await;

            // Periodically check cache hit rate (every 100 operations)
            if collector.count().await % 100 == 0 {
                collector.check_cache_hit_rate_warning().await;
            }
        }

        Ok(result)
    }

    /// Evaluate a single policy against the context
    pub(crate) async fn evaluate_policy(
        &self,
        policy: &Policy,
        context: &PolicyEvaluationContext,
    ) -> Result<EvaluationDetail, AppError> {
        // Parse policy target and rules
        let target = policy.parse_target().map_err(|e| {
            AppError::InternalServerError(format!("Failed to parse policy target: {}", e))
        })?;
        let rules = policy.parse_rules().map_err(|e| {
            AppError::InternalServerError(format!("Failed to parse policy rules: {}", e))
        })?;

        // Check if policy target matches the request
        let target_matches = self.evaluate_target(&target, context)?;
        if !target_matches {
            return Ok(EvaluationDetail {
                policy_id: policy.id.clone(),
                policy_name: policy.name.clone(),
                matched: false,
                effect: policy.effect,
                rule_results: Vec::new(),
            });
        }

        // Evaluate all rules
        let mut rule_results = Vec::new();
        let mut any_rule_matched = false;

        for (index, rule) in rules.iter().enumerate() {
            let rule_result = self.rule_evaluator.evaluate_rule(rule, context)?;
            if rule_result.condition_result {
                any_rule_matched = true;
            }
            rule_results.push(RuleEvaluationResult {
                rule_index: index,
                condition_result: rule_result.condition_result,
                effect: rule_result.effect,
                error: rule_result.error,
            });
        }

        Ok(EvaluationDetail {
            policy_id: policy.id.clone(),
            policy_name: policy.name.clone(),
            matched: target_matches && any_rule_matched,
            effect: policy.effect,
            rule_results,
        })
    }

    /// Evaluate policy target against context
    pub(crate) fn evaluate_target(
        &self,
        target: &Target,
        context: &PolicyEvaluationContext,
    ) -> Result<bool, AppError> {
        // Check action match
        if !target.actions.is_empty() {
            let action_matches = target
                .actions
                .iter()
                .any(|action_pattern| self.match_pattern(action_pattern, &context.action));
            if !action_matches {
                return Ok(false);
            }
        }

        // NOTE: Evaluating subjects, resources, and environment as string expressions
        // is a complex task requiring a dedicated parser. This functionality was
        // removed and is pending re-implementation. For now, we only match actions.
        // A full implementation would parse these strings and evaluate them.

        Ok(true)
    }
}
