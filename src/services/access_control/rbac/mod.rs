use crate::{
    config::access_control::AccessControlConfig,
    errors::{
        access_control::{AccessControlError, AccessControlResult},
        context::ErrorContext,
    },
    models::entities::{admin_user::AdminUser, permission::Permission, role::Role as RoleEntity},
    repositories::{
        permission::PermissionRepository, user::UserRepository, role::RoleRepository,
    },
    services::{
        access_control::cache::{AccessControlCache, CacheManager},
        metrics::{MetricsCollector, PerformanceMetrics},
    },
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{debug, info};

mod cache;
mod evaluator;
mod repository;

/// Enhanced RBAC Engine with role inheritance and caching
#[derive(Clone)]
pub struct RbacEngine {
    permission_repo: Arc<PermissionRepository>,
    user_repo: Arc<UserRepository>,
    role_repo: Arc<RoleRepository>,
    cache: Option<Arc<dyn AccessControlCache>>,
    config: AccessControlConfig,
    metrics_collector: Option<Arc<MetricsCollector>>,
}

/// RBAC evaluation request
#[derive(Debug, Clone)]
pub struct RbacRequest {
    pub user_id: i64,
    pub resource: String,
    pub action: String,
    pub resource_type: Option<String>,
}

/// RBAC evaluation result
#[derive(Debug, Clone, serde::Serialize)]
pub struct RbacResult {
    pub decision: RbacDecision,
    pub matched_permissions: Vec<String>,
    pub user_roles: Vec<String>,
    pub evaluation_time: Duration,
    pub cache_hit: bool,
}

/// RBAC decision
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum RbacDecision {
    Permit,
    Deny,
    NotApplicable,
}

/// Role information with inheritance
#[derive(Debug, Clone)]
pub struct Role {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub permissions: Vec<Permission>,
    pub parent_roles: Vec<i64>,
    pub is_active: bool,
}

/// User with resolved roles and permissions
#[derive(Debug, Clone)]
pub struct UserWithRoles {
    pub user: AdminUser,
    pub role: RoleEntity,
    pub effective_permissions: Vec<Permission>,
    pub role_hierarchy: Vec<String>,
}

/// RBAC cache statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct RbacCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub user_permissions_count: usize,
    pub permission_checks_count: usize,
    pub user_roles_count: usize,
}

impl RbacEngine {
    pub fn new(
        permission_repo: PermissionRepository,
        user_repo: UserRepository,
        role_repo: RoleRepository,
        config: AccessControlConfig,
    ) -> Self {
        Self {
            permission_repo: Arc::new(permission_repo),
            user_repo: Arc::new(user_repo),
            role_repo: Arc::new(role_repo),
            cache: None,
            config,
            metrics_collector: None,
        }
    }

    /// Create RBAC engine with cache manager
    pub fn with_cache_manager(
        permission_repo: PermissionRepository,
        user_repo: UserRepository,
        role_repo: RoleRepository,
        config: AccessControlConfig,
        cache_manager: CacheManager,
    ) -> Self {
        Self {
            permission_repo: Arc::new(permission_repo),
            user_repo: Arc::new(user_repo),
            role_repo: Arc::new(role_repo),
            cache: Some(Arc::new(cache_manager)),
            config,
            metrics_collector: None,
        }
    }

    /// Create RBAC engine with any cache implementation
    pub fn with_cache(
        permission_repo: PermissionRepository,
        user_repo: UserRepository,
        role_repo: RoleRepository,
        config: AccessControlConfig,
        cache: Arc<dyn AccessControlCache>,
    ) -> Self {
        Self {
            permission_repo: Arc::new(permission_repo),
            user_repo: Arc::new(user_repo),
            role_repo: Arc::new(role_repo),
            cache: Some(cache),
            config,
            metrics_collector: None,
        }
    }

    /// Set metrics collector for performance monitoring
    pub fn with_metrics_collector(mut self, metrics_collector: Arc<MetricsCollector>) -> Self {
        self.metrics_collector = Some(metrics_collector);
        self
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> RbacCacheStats {
        if let Some(cache) = &self.cache {
            let stats = cache.get_stats().await;
            RbacCacheStats {
                hits: stats.hits,
                misses: stats.misses,
                evictions: stats.evictions,
                user_permissions_count: stats.size,
                permission_checks_count: stats.size,
                user_roles_count: 0, // Not tracked separately in unified cache
            }
        } else {
            RbacCacheStats {
                hits: 0,
                misses: 0,
                evictions: 0,
                user_permissions_count: 0,
                permission_checks_count: 0,
                user_roles_count: 0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rbac_decision_enum() {
        assert_eq!(RbacDecision::Permit, RbacDecision::Permit);
        assert_ne!(RbacDecision::Permit, RbacDecision::Deny);
    }

    #[tokio::test]
    async fn test_rbac_request_creation() {
        let request = RbacRequest {
            user_id: 1,
            resource: "test_resource".to_string(),
            action: "read".to_string(),
            resource_type: Some("document".to_string()),
        };

        assert_eq!(request.user_id, 1);
        assert_eq!(request.resource, "test_resource");
        assert_eq!(request.action, "read");
    }

    #[tokio::test]
    async fn test_rbac_result_serialization() {
        let result = RbacResult {
            decision: RbacDecision::Permit,
            matched_permissions: vec!["read".to_string()],
            user_roles: vec!["admin".to_string()],
            evaluation_time: Duration::from_millis(10),
            cache_hit: false,
        };

        assert_eq!(result.decision, RbacDecision::Permit);
        assert_eq!(result.matched_permissions.len(), 1);
        assert!(!result.cache_hit);
    }
}
