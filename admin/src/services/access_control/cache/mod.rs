//! Cache implementations for the access control system
//!
//! This module provides multiple cache implementations optimized for different use cases:
//!
//! - **LRU Cache**: Traditional LRU cache with RwLock protection. Good for general use.
//! - **DashMap Cache**: High-performance concurrent cache using DashMap. Optimized for
//!   read-heavy workloads with minimal lock contention. Best for frequently accessed
//!   data like user permissions and policy evaluations.
//! - **Cache Manager**: Multi-level cache manager that coordinates different cache types.
//! - **Invalidation Manager**: Handles cache invalidation events with configurable strategies.
//! - **Background Tasks**: Automated cleanup and maintenance tasks.
//!
//! ## Performance Characteristics
//!
//! ### AsyncLruCache (RwLock-based)
//! - Read operations: Requires read lock acquisition
//! - Write operations: Requires write lock acquisition (blocks all readers)
//! - Best for: Balanced read/write workloads, smaller caches
//!
//! ### DashMapCache (Lock-free)
//! - Read operations: Lock-free, highly concurrent
//! - Write operations: Fine-grained locking per shard
//! - Best for: Read-heavy workloads (>90% reads), high concurrency scenarios
//!
//! ## Usage Guidelines
//!
//! Use `DashMapCache` when:
//! - Cache is accessed by many concurrent threads
//! - Read operations significantly outnumber writes (>10:1 ratio)
//! - Low latency is critical
//!
//! Use `AsyncLruCache` when:
//! - Cache size is small (<1000 entries)
//! - Write operations are frequent
//! - LRU eviction order is important

mod implementation;
pub mod invalidation_events;

pub use implementation::{
    AbacCacheKey,
    AsyncLruCache,

    BackgroundTaskConfig,
    BackgroundTaskStats,
    // Background tasks
    CacheBackgroundTaskManager,
    CacheConfig,
    // Invalidation
    CacheInvalidationManager,
    // Cache manager
    CacheManager,
    CacheManagerStats,
    // Cache statistics
    CacheStats,
    CacheStatsSnapshot,

    CacheWarmUpRequest,
    // DashMap cache
    DashMapCache,

    InvalidationConfig,
    InvalidationStats,

    InvalidationStrategy,
    // LRU cache
    LruCache,
    MaintenanceResult,
    PermissionCacheKey,
    ResourceCacheKey,

    WarmUpResult,
};

// Re-export invalidation events
pub use invalidation_events::{InvalidationEvent, InvalidationScope};

use crate::{
    models::entities::permission::Permission,
    services::access_control::rbac::{RbacRequest, RbacResult},
};
use async_trait::async_trait;

/// Unified cache interface for access control system
///
/// This trait provides a consistent interface for caching access control
/// evaluation results, user permissions, and related data. It abstracts
/// the underlying cache implementation to allow for different strategies
/// (in-memory, distributed, etc.) while maintaining a consistent API.
#[async_trait]
pub trait AccessControlCache: Send + Sync {
    /// Get cached permission evaluation result
    ///
    /// # Arguments
    /// * `request` - The RBAC request to look up in cache
    ///
    /// # Returns
    /// * `Some(RbacResult)` if cached result exists and is valid
    /// * `None` if no cached result or cache is disabled
    async fn get_permission_result(&self, request: &RbacRequest) -> Option<RbacResult>;

    /// Cache permission evaluation result
    ///
    /// # Arguments
    /// * `request` - The RBAC request that was evaluated
    /// * `result` - The evaluation result to cache
    async fn cache_permission_result(&self, request: &RbacRequest, result: &RbacResult);

    /// Get cached user permissions
    ///
    /// # Arguments
    /// * `user_id` - The user ID to look up
    ///
    /// # Returns
    /// * `Some(Vec<Permission>)` if cached permissions exist and are valid
    /// * `None` if no cached permissions or cache is disabled
    async fn get_user_permissions(&self, user_id: i64) -> Option<Vec<Permission>>;

    /// Cache user permissions
    ///
    /// # Arguments
    /// * `user_id` - The user ID to cache permissions for
    /// * `permissions` - The permissions to cache
    async fn cache_user_permissions(&self, user_id: i64, permissions: &[Permission]);

    /// Invalidate all cache entries for a specific user
    ///
    /// This should remove:
    /// - User permissions cache
    /// - Permission evaluation results for this user
    /// - Any other user-specific cached data
    ///
    /// # Arguments
    /// * `user_id` - The user ID to invalidate cache for
    async fn invalidate_user_cache(&self, user_id: i64);

    /// Clear all caches
    ///
    /// This removes all cached data across all cache levels.
    async fn clear_all(&self);

    /// Get cache statistics
    ///
    /// Returns aggregated statistics about cache performance including
    /// hits, misses, evictions, and hit rate.
    ///
    /// # Returns
    /// * `CacheStatsSnapshot` - Aggregated cache statistics snapshot
    async fn get_stats(&self) -> CacheStatsSnapshot;
}
