use super::*;

impl RbacEngine {
    /// Evaluate RBAC permission for a user
    pub async fn evaluate(&self, request: &RbacRequest) -> AccessControlResult<RbacResult> {
        let start_time = Instant::now();
        let cache_hit = false;

        debug!(
            user_id = request.user_id,
            resource = %request.resource,
            action = %request.action,
            resource_type = ?request.resource_type,
            "Evaluating RBAC permission"
        );

        // Check cache first if enabled
        if self.config.enable_caching {
            if let Some(cache) = &self.cache {
                if let Some(cached_result) = cache.get_permission_result(request).await {
                    debug!(user_id = request.user_id, "RBAC cache hit");

                    // Record metrics for cache hit
                    if let Some(collector) = &self.metrics_collector {
                        let metric = PerformanceMetrics::new("rbac_evaluate")
                            .with_duration(start_time.elapsed().as_secs_f64() * 1000.0)
                            .with_cache_hit(true)
                            .with_db_queries(0);
                        collector.record(metric).await;
                    }

                    return Ok(cached_result);
                }
            }
        }

        // Perform actual evaluation
        let result = self.evaluate_internal(request, start_time).await?;

        // Cache the result if enabled
        if self.config.enable_caching {
            if let Some(cache) = &self.cache {
                cache.cache_permission_result(request, &result).await;
            }
        }

        // Record metrics
        if let Some(collector) = &self.metrics_collector {
            let metric = PerformanceMetrics::new("rbac_evaluate")
                .with_duration(start_time.elapsed().as_secs_f64() * 1000.0)
                .with_cache_hit(cache_hit)
                .with_db_queries(2);
            collector.record(metric).await;

            // Periodically check cache hit rate (every 100 operations)
            if collector.count().await % 100 == 0 {
                collector.check_cache_hit_rate_warning().await;
            }
        }

        Ok(result)
    }

    /// Internal evaluation logic
    pub(super) async fn evaluate_internal(
        &self,
        request: &RbacRequest,
        start_time: Instant,
    ) -> AccessControlResult<RbacResult> {
        // Get user with roles and permissions
        let user_with_roles = self.get_user_with_roles(request.user_id).await?;

        // Check if user is active
        if !user_with_roles.user.is_active {
            return Ok(RbacResult {
                decision: RbacDecision::Deny,
                matched_permissions: vec![],
                user_roles: vec![user_with_roles.role.name.clone()],
                evaluation_time: start_time.elapsed(),
                cache_hit: false,
            });
        }

        // Find matching permissions
        let matched_permissions = self
            .find_matching_permissions(
                &user_with_roles.effective_permissions,
                &request.resource,
                &request.action,
                request.resource_type.as_deref(),
            )
            .await?;

        let decision = if matched_permissions.is_empty() {
            RbacDecision::Deny
        } else {
            RbacDecision::Permit
        };

        let result = RbacResult {
            decision,
            matched_permissions: matched_permissions.iter().map(|p| p.code.clone()).collect(),
            user_roles: user_with_roles.role_hierarchy,
            evaluation_time: start_time.elapsed(),
            cache_hit: false,
        };

        debug!(
            user_id = request.user_id,
            decision = ?result.decision,
            evaluation_time_ms = result.evaluation_time.as_millis(),
            matched_permissions = result.matched_permissions.len(),
            cache_hit = result.cache_hit,
            "RBAC evaluation completed"
        );

        Ok(result)
    }

    /// Find permissions that match the resource and action
    pub(super) async fn find_matching_permissions(
        &self,
        permissions: &[Permission],
        resource: &str,
        action: &str,
        resource_type: Option<&str>,
    ) -> AccessControlResult<Vec<Permission>> {
        let mut matched = Vec::new();

        for permission in permissions {
            if self
                .permission_matches(permission, resource, action, resource_type)
                .await?
            {
                matched.push(permission.clone());
            }
        }

        Ok(matched)
    }

    /// Check if a permission matches the resource and action
    pub(super) async fn permission_matches(
        &self,
        permission: &Permission,
        resource: &str,
        action: &str,
        _resource_type: Option<&str>,
    ) -> AccessControlResult<bool> {
        // Simple RBAC matching based on code
        // Format expected: "resource:action" or "*" or "resource:*"
        
        let code = &permission.code;
        
        // 1. Super admin wildcard
        if code == "*" {
            return Ok(true);
        }
        
        // 2. Exact match "resource:action"
        let required_permission = format!("{}:{}", resource, action);
        if code == &required_permission {
            return Ok(true);
        }
        
        // 3. Resource wildcard "resource:*"
        let resource_wildcard = format!("{}:*", resource);
        if code == &resource_wildcard {
            return Ok(true);
        }

        // 4. Fallback: Check if code exactly matches the resource (e.g. "dashboard" permission for "dashboard" resource)
        if code == resource {
             return Ok(true);
        }

        Ok(false)
    }

    /// Check if user has a specific permission
    pub async fn has_permission(
        &self,
        user_id: i64,
        permission_name: &str,
    ) -> AccessControlResult<bool> {
        // For has_permission, we treat permission_name as the full code we are looking for.
        // We split it to resource/action if possible, or just use it as resource.
        let parts: Vec<&str> = permission_name.split(':').collect();
        let (resource, action) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            (permission_name, "access")
        };

        let request = RbacRequest {
            user_id,
            resource: resource.to_string(),
            action: action.to_string(),
            resource_type: None,
        };

        let result = self.evaluate(&request).await?;
        Ok(result.decision == RbacDecision::Permit)
    }
}
