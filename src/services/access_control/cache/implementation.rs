//! Consolidated cache implementation for access control system
//!
//! This module contains all cache implementation details including:
//! - LRU cache with TTL support
//! - DashMap-based concurrent cache
//! - Multi-level cache manager
//! - Cache invalidation logic
//! - Background cleanup tasks

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::{Hash, Hasher},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{Notify, RwLock},
    task::JoinHandle,
    time::interval,
};
use tracing::{debug, error, info, warn};

use crate::{
    config::access_control::AccessControlConfig,
    errors::access_control::AccessControlResult,
    models::entities::{
        abac::{AccessRequest, AccessResponse, AttributeValue, Policy},
        permission::Permission,
    },
    services::access_control::rbac::{RbacRequest, RbacResult},
};

use super::{AccessControlCache, InvalidationEvent};

// ============================================================================
// Cache Statistics
// ============================================================================

/// Cache statistics with atomic counters for lock-free updates
#[derive(Debug)]
pub struct CacheStats {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub evictions: AtomicU64,
    pub expired_evictions: AtomicU64,
    pub size: AtomicUsize,
    pub capacity: usize,
}

impl CacheStats {
    /// Create new cache statistics
    pub fn new(capacity: usize) -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            expired_evictions: AtomicU64::new(0),
            size: AtomicUsize::new(0),
            capacity,
        }
    }

    /// Increment hits counter
    pub fn increment_hits(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment misses counter
    pub fn increment_misses(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment evictions counter
    pub fn increment_evictions(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment expired evictions counter
    pub fn increment_expired_evictions(&self) {
        self.expired_evictions.fetch_add(1, Ordering::Relaxed);
    }

    /// Update size
    pub fn set_size(&self, size: usize) {
        self.size.store(size, Ordering::Relaxed);
    }

    /// Get current hit rate
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Get snapshot of statistics for serialization
    pub fn snapshot(&self) -> CacheStatsSnapshot {
        CacheStatsSnapshot {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            expired_evictions: self.expired_evictions.load(Ordering::Relaxed),
            size: self.size.load(Ordering::Relaxed),
            capacity: self.capacity,
            hit_rate: self.hit_rate(),
        }
    }

    /// Reset all counters
    pub fn reset(&self) {
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.evictions.store(0, Ordering::Relaxed);
        self.expired_evictions.store(0, Ordering::Relaxed);
        self.size.store(0, Ordering::Relaxed);
    }
}

/// Serializable snapshot of cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStatsSnapshot {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub expired_evictions: u64,
    pub size: usize,
    pub capacity: usize,
    pub hit_rate: f64,
}

// ============================================================================
// LRU Cache Implementation
// ============================================================================

/// Cache entry with metadata
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CacheEntry<V> {
    value: V,
    created_at: Instant,
    expires_at: Instant,
    access_count: u64,
    last_accessed: Instant,
}

/// Generic LRU cache with TTL support
#[derive(Debug)]
pub struct LruCache<K, V>
where
    K: Clone + Hash + Eq + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Maximum number of entries
    capacity: usize,
    /// Cache entries with metadata
    entries: HashMap<K, CacheEntry<V>>,
    /// Access order queue (most recently used at back)
    access_order: VecDeque<K>,
    /// Default TTL for entries
    default_ttl: Duration,
    /// Cache statistics
    stats: CacheStats,
}

impl<K, V> LruCache<K, V>
where
    K: Clone + Hash + Eq + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Create a new LRU cache with specified capacity and default TTL
    pub fn new(capacity: usize, default_ttl: Duration) -> Self {
        Self {
            capacity,
            entries: HashMap::with_capacity(capacity),
            access_order: VecDeque::with_capacity(capacity),
            default_ttl,
            stats: CacheStats::new(capacity),
        }
    }

    /// Get a value from the cache
    pub fn get(&mut self, key: &K) -> Option<V> {
        let now = Instant::now();

        // Check if entry exists and is not expired
        let (should_remove, value) = if let Some(entry) = self.entries.get(key) {
            if now > entry.expires_at {
                (true, None)
            } else {
                (false, Some(entry.value.clone()))
            }
        } else {
            (false, None)
        };

        if should_remove {
            debug!("Cache entry expired for key");
            self.remove_entry(key);
            self.stats.increment_expired_evictions();
            self.stats.increment_misses();
            return None;
        }

        if let Some(value) = value {
            // Update access metadata
            if let Some(entry) = self.entries.get_mut(key) {
                entry.last_accessed = now;
                entry.access_count += 1;
            }

            // Move to end of access order (most recently used)
            self.move_to_end(key);

            self.stats.increment_hits();

            debug!("Cache hit for key");
            Some(value)
        } else {
            self.stats.increment_misses();
            debug!("Cache miss for key");
            None
        }
    }

    /// Insert a value into the cache with default TTL
    pub fn insert(&mut self, key: K, value: V) {
        self.insert_with_ttl(key, value, self.default_ttl);
    }

    /// Insert a value into the cache with custom TTL
    pub fn insert_with_ttl(&mut self, key: K, value: V, ttl: Duration) {
        let now = Instant::now();

        // If key already exists, update it
        if self.entries.contains_key(&key) {
            self.entries.insert(
                key.clone(),
                CacheEntry {
                    value,
                    created_at: now,
                    expires_at: now + ttl,
                    access_count: 1,
                    last_accessed: now,
                },
            );
            self.move_to_end(&key);
            return;
        }

        // Check if we need to evict entries
        while self.entries.len() >= self.capacity {
            self.evict_lru();
        }

        // Insert new entry
        self.entries.insert(
            key.clone(),
            CacheEntry {
                value,
                created_at: now,
                expires_at: now + ttl,
                access_count: 1,
                last_accessed: now,
            },
        );

        self.access_order.push_back(key);
        self.stats.set_size(self.entries.len());

        debug!("Inserted new cache entry");
    }

    /// Remove a specific key from the cache
    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(entry) = self.remove_entry(key) {
            Some(entry.value)
        } else {
            None
        }
    }

    /// Clear all entries from the cache
    pub fn clear(&mut self) {
        let size = self.entries.len();
        self.entries.clear();
        self.access_order.clear();
        self.stats.set_size(0);
        // Add cleared entries to evictions count
        for _ in 0..size {
            self.stats.increment_evictions();
        }
        debug!("Cache cleared");
    }

    /// Get cache statistics snapshot
    pub fn stats(&self) -> CacheStatsSnapshot {
        self.stats.snapshot()
    }

    /// Check if cache contains a key (without updating access order)
    pub fn contains_key(&self, key: &K) -> bool {
        if let Some(entry) = self.entries.get(key) {
            Instant::now() <= entry.expires_at
        } else {
            false
        }
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clean up expired entries
    pub fn cleanup_expired(&mut self) -> usize {
        let now = Instant::now();
        let mut expired_keys = Vec::new();

        // Find expired keys
        for (key, entry) in &self.entries {
            if now > entry.expires_at {
                expired_keys.push(key.clone());
            }
        }

        // Remove expired entries
        let expired_count = expired_keys.len();
        for key in expired_keys {
            self.remove_entry(&key);
            self.stats.increment_expired_evictions();
        }

        if expired_count > 0 {
            debug!("Cleaned up {} expired cache entries", expired_count);
        }

        expired_count
    }

    /// Get keys that match a predicate
    pub fn keys_matching<F>(&self, predicate: F) -> Vec<K>
    where
        F: Fn(&K) -> bool,
    {
        self.entries
            .keys()
            .filter(|k| predicate(k))
            .cloned()
            .collect()
    }

    /// Remove entries matching a predicate
    pub fn remove_matching<F>(&mut self, predicate: F) -> usize
    where
        F: Fn(&K) -> bool,
    {
        let keys_to_remove: Vec<K> = self
            .entries
            .keys()
            .filter(|k| predicate(k))
            .cloned()
            .collect();

        let removed_count = keys_to_remove.len();
        for key in keys_to_remove {
            self.remove_entry(&key);
        }

        debug!("Removed {} cache entries matching predicate", removed_count);
        removed_count
    }

    /// Internal method to remove an entry
    fn remove_entry(&mut self, key: &K) -> Option<CacheEntry<V>> {
        if let Some(entry) = self.entries.remove(key) {
            // Remove from access order
            if let Some(pos) = self.access_order.iter().position(|k| k == key) {
                self.access_order.remove(pos);
            }
            self.stats.set_size(self.entries.len());
            Some(entry)
        } else {
            None
        }
    }

    /// Move key to end of access order (most recently used)
    fn move_to_end(&mut self, key: &K) {
        // Remove from current position
        if let Some(pos) = self.access_order.iter().position(|k| k == key) {
            self.access_order.remove(pos);
        }
        // Add to end
        self.access_order.push_back(key.clone());
    }

    /// Evict least recently used entry
    fn evict_lru(&mut self) {
        if let Some(lru_key) = self.access_order.pop_front() {
            self.entries.remove(&lru_key);
            self.stats.increment_evictions();
            self.stats.set_size(self.entries.len());
            debug!("Evicted LRU cache entry");
        }
    }
}

/// Thread-safe wrapper around LRU cache
pub struct AsyncLruCache<K, V>
where
    K: Clone + Hash + Eq + Send + Sync,
    V: Clone + Send + Sync,
{
    cache: Arc<RwLock<LruCache<K, V>>>,
}

impl<K, V> AsyncLruCache<K, V>
where
    K: Clone + Hash + Eq + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Create a new async LRU cache
    pub fn new(capacity: usize, default_ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(capacity, default_ttl))),
        }
    }

    /// Get a value from the cache
    pub async fn get(&self, key: &K) -> Option<V> {
        let mut cache = self.cache.write().await;
        cache.get(key)
    }

    /// Insert a value into the cache
    pub async fn insert(&self, key: K, value: V) {
        let mut cache = self.cache.write().await;
        cache.insert(key, value);
    }

    /// Insert a value with custom TTL
    pub async fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) {
        let mut cache = self.cache.write().await;
        cache.insert_with_ttl(key, value, ttl);
    }

    /// Remove a key from the cache
    pub async fn remove(&self, key: &K) -> Option<V> {
        let mut cache = self.cache.write().await;
        cache.remove(key)
    }

    /// Clear all entries
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get cache statistics snapshot
    pub async fn stats(&self) -> CacheStatsSnapshot {
        let cache = self.cache.read().await;
        cache.stats()
    }

    /// Check if cache contains a key
    pub async fn contains_key(&self, key: &K) -> bool {
        let cache = self.cache.read().await;
        cache.contains_key(key)
    }

    /// Get current cache size
    pub async fn len(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }

    /// Check if cache is empty
    pub async fn is_empty(&self) -> bool {
        let cache = self.cache.read().await;
        cache.is_empty()
    }

    /// Clean up expired entries
    pub async fn cleanup_expired(&self) -> usize {
        let mut cache = self.cache.write().await;
        cache.cleanup_expired()
    }

    /// Remove entries matching a predicate
    pub async fn remove_matching<F>(&self, predicate: F) -> usize
    where
        F: Fn(&K) -> bool,
    {
        let mut cache = self.cache.write().await;
        cache.remove_matching(predicate)
    }

    /// Get keys matching a predicate
    pub async fn keys_matching<F>(&self, predicate: F) -> Vec<K>
    where
        F: Fn(&K) -> bool,
    {
        let cache = self.cache.read().await;
        cache.keys_matching(predicate)
    }
}

impl<K, V> Clone for AsyncLruCache<K, V>
where
    K: Clone + Hash + Eq + Send + Sync,
    V: Clone + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
        }
    }
}

// ============================================================================
// DashMap Cache Implementation
// ============================================================================

/// Cache entry with expiration metadata for DashMap
#[derive(Debug, Clone)]
struct DashMapCacheEntry<V> {
    value: V,
    expires_at: Instant,
    created_at: Instant,
}

/// High-performance concurrent cache using DashMap
/// Optimized for read-heavy workloads with minimal lock contention
pub struct DashMapCache<K, V>
where
    K: Clone + Hash + Eq + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Main cache storage using DashMap for lock-free reads
    entries: DashMap<K, DashMapCacheEntry<V>>,
    /// Maximum capacity
    capacity: usize,
    /// Default TTL for entries
    default_ttl: Duration,
    /// Cache statistics
    stats: Arc<CacheStats>,
}

impl<K, V> DashMapCache<K, V>
where
    K: Clone + Hash + Eq + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Create a new DashMap-based cache
    pub fn new(capacity: usize, default_ttl: Duration) -> Self {
        Self {
            entries: DashMap::with_capacity(capacity),
            capacity,
            default_ttl,
            stats: Arc::new(CacheStats::new(capacity)),
        }
    }

    /// Get a value from the cache (lock-free read)
    pub fn get(&self, key: &K) -> Option<V> {
        let now = Instant::now();

        // Try to get the entry
        if let Some(entry_ref) = self.entries.get(key) {
            let entry = entry_ref.value();

            // Check if expired
            if now > entry.expires_at {
                // Drop the read lock before removing
                drop(entry_ref);

                // Remove expired entry
                self.entries.remove(key);
                self.stats.increment_expired_evictions();
                self.stats.increment_misses();
                self.stats.set_size(self.entries.len());

                debug!("Cache entry expired for key");
                return None;
            }

            // Cache hit
            self.stats.increment_hits();
            debug!("Cache hit for key");
            Some(entry.value.clone())
        } else {
            // Cache miss
            self.stats.increment_misses();
            debug!("Cache miss for key");
            None
        }
    }

    /// Insert a value into the cache with default TTL
    pub fn insert(&self, key: K, value: V) {
        self.insert_with_ttl(key, value, self.default_ttl);
    }

    /// Insert a value into the cache with custom TTL
    pub fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) {
        let now = Instant::now();

        let entry = DashMapCacheEntry {
            value,
            expires_at: now + ttl,
            created_at: now,
        };

        // Check capacity before inserting
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            // Need to evict - find and remove oldest entry
            self.evict_oldest();
        }

        // Insert or update entry
        self.entries.insert(key, entry);
        self.stats.set_size(self.entries.len());

        debug!("Inserted cache entry");
    }

    /// Remove a specific key from the cache
    pub fn remove(&self, key: &K) -> Option<V> {
        self.entries.remove(key).map(|(_, entry)| {
            self.stats.set_size(self.entries.len());
            entry.value
        })
    }

    /// Clear all entries from the cache
    pub fn clear(&self) {
        let size = self.entries.len();
        self.entries.clear();
        self.stats.set_size(0);

        // Add cleared entries to evictions count
        for _ in 0..size {
            self.stats.increment_evictions();
        }

        debug!("Cache cleared");
    }

    /// Get cache statistics snapshot
    pub fn stats(&self) -> CacheStatsSnapshot {
        self.stats.snapshot()
    }

    /// Check if cache contains a key
    pub fn contains_key(&self, key: &K) -> bool {
        if let Some(entry_ref) = self.entries.get(key) {
            Instant::now() <= entry_ref.expires_at
        } else {
            false
        }
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clean up expired entries
    pub fn cleanup_expired(&self) -> usize {
        let now = Instant::now();

        // Collect expired keys
        let expired_keys: Vec<K> = self
            .entries
            .iter()
            .filter_map(|entry| {
                if now > entry.value().expires_at {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();

        // Remove expired entries
        let expired_count = expired_keys.len();
        for key in expired_keys {
            self.entries.remove(&key);
            self.stats.increment_expired_evictions();
        }

        if expired_count > 0 {
            self.stats.set_size(self.entries.len());
            debug!("Cleaned up {} expired cache entries", expired_count);
        }

        expired_count
    }

    /// Remove entries matching a predicate
    pub fn remove_matching<F>(&self, predicate: F) -> usize
    where
        F: Fn(&K) -> bool,
    {
        let keys_to_remove: Vec<K> = self
            .entries
            .iter()
            .filter_map(|entry| {
                if predicate(entry.key()) {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();

        let removed_count = keys_to_remove.len();
        for key in keys_to_remove {
            self.entries.remove(&key);
        }

        if removed_count > 0 {
            self.stats.set_size(self.entries.len());
            debug!("Removed {} cache entries matching predicate", removed_count);
        }

        removed_count
    }

    /// Evict oldest entry based on creation time
    fn evict_oldest(&self) {
        // Find the oldest entry
        let oldest_key = self
            .entries
            .iter()
            .min_by_key(|entry| entry.value().created_at)
            .map(|entry| entry.key().clone());

        if let Some(key) = oldest_key {
            self.entries.remove(&key);
            self.stats.increment_evictions();
            self.stats.set_size(self.entries.len());
            debug!("Evicted oldest cache entry");
        }
    }
}

impl<K, V> Clone for DashMapCache<K, V>
where
    K: Clone + Hash + Eq + Send + Sync,
    V: Clone + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            capacity: self.capacity,
            default_ttl: self.default_ttl,
            stats: Arc::clone(&self.stats),
        }
    }
}

// ============================================================================
// Cache Manager - Multi-level Cache Coordination
// ============================================================================

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Enable/disable caching
    pub enabled: bool,

    /// L1 cache settings (evaluation results)
    pub l1_capacity: usize,
    pub l1_ttl: Duration,

    /// L2 cache settings (user data)
    pub l2_capacity: usize,
    pub l2_ttl: Duration,

    /// L3 cache settings (resource data)
    pub l3_capacity: usize,
    pub l3_ttl: Duration,

    /// Cleanup interval
    pub cleanup_interval: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            l1_capacity: 10000,
            l1_ttl: Duration::from_secs(300), // 5 minutes
            l2_capacity: 5000,
            l2_ttl: Duration::from_secs(1800), // 30 minutes
            l3_capacity: 2000,
            l3_ttl: Duration::from_secs(3600),          // 1 hour
            cleanup_interval: Duration::from_secs(300), // 5 minutes
        }
    }
}

impl From<&AccessControlConfig> for CacheConfig {
    fn from(config: &AccessControlConfig) -> Self {
        Self {
            enabled: config.enable_caching,
            l1_ttl: Duration::from_secs(config.cache_ttl_seconds),
            l2_ttl: Duration::from_secs(config.cache_ttl_seconds * 2),
            l3_ttl: Duration::from_secs(config.cache_ttl_seconds * 4),
            ..Default::default()
        }
    }
}

/// Cache key for permission evaluations
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PermissionCacheKey {
    pub user_id: i64,
    pub resource: String,
    pub action: String,
    pub resource_type: Option<String>,
}

impl From<&RbacRequest> for PermissionCacheKey {
    fn from(request: &RbacRequest) -> Self {
        Self {
            user_id: request.user_id,
            resource: request.resource.clone(),
            action: request.action.clone(),
            resource_type: request.resource_type.clone(),
        }
    }
}

/// Cache key for ABAC evaluations
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AbacCacheKey {
    pub subject_id: String,
    pub resource_id: String,
    pub resource_type: String,
    pub action: String,
    pub context_hash: u64,
}

impl From<&AccessRequest> for AbacCacheKey {
    fn from(request: &AccessRequest) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        // Hash context attributes for cache key
        for (key, value) in &request.context.environment {
            key.hash(&mut hasher);
            // Simple hash of attribute value
            match value {
                AttributeValue::String(s) => s.hash(&mut hasher),
                AttributeValue::Number(n) => n.to_bits().hash(&mut hasher),
                AttributeValue::Boolean(b) => b.hash(&mut hasher),
                AttributeValue::Array(arr) => arr.len().hash(&mut hasher),
                AttributeValue::Object(obj) => obj.len().hash(&mut hasher),
                AttributeValue::Null => 0u8.hash(&mut hasher),
            }
        }

        Self {
            subject_id: request.subject.id.clone(),
            resource_id: request.resource.id.clone(),
            resource_type: request.resource.resource_type.clone(),
            action: request.action.name.clone(),
            context_hash: hasher.finish(),
        }
    }
}

/// Cache key for resource attributes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceCacheKey {
    pub resource_id: String,
    pub resource_type: String,
}

/// Cache manager statistics
#[derive(Debug, Clone, Serialize)]
pub struct CacheManagerStats {
    pub permission_cache_stats: CacheStatsSnapshot,
    pub abac_cache_stats: CacheStatsSnapshot,
    pub user_permissions_cache_stats: CacheStatsSnapshot,
    pub policy_cache_stats: CacheStatsSnapshot,
    pub resource_attributes_cache_stats: CacheStatsSnapshot,
    pub total_memory_usage_estimate: usize,
    pub cache_effectiveness: f64,
}

/// Multi-level cache manager for access control system
#[derive(Clone)]
#[allow(dead_code)]
pub struct CacheManager {
    /// L1 Cache: Permission evaluation results
    permission_cache: AsyncLruCache<PermissionCacheKey, RbacResult>,

    /// L1 Cache: ABAC policy evaluation results
    abac_cache: AsyncLruCache<AbacCacheKey, AccessResponse>,

    /// L2 Cache: User permissions
    user_permissions_cache: AsyncLruCache<i64, Vec<Permission>>,

    /// L2 Cache: ABAC policies
    policy_cache: AsyncLruCache<String, Policy>,

    /// L3 Cache: Resource attributes
    resource_attributes_cache: AsyncLruCache<ResourceCacheKey, HashMap<String, serde_json::Value>>,

    /// Cache configuration
    config: CacheConfig,

    /// Cache statistics aggregator
    stats: Arc<RwLock<CacheManagerStats>>,
}

impl CacheManager {
    /// Create a new cache manager
    pub fn new(config: CacheConfig) -> Self {
        Self {
            permission_cache: AsyncLruCache::new(config.l1_capacity, config.l1_ttl),
            abac_cache: AsyncLruCache::new(config.l1_capacity, config.l1_ttl),
            user_permissions_cache: AsyncLruCache::new(config.l2_capacity, config.l2_ttl),
            policy_cache: AsyncLruCache::new(config.l2_capacity, config.l2_ttl),
            resource_attributes_cache: AsyncLruCache::new(config.l3_capacity, config.l3_ttl),
            config,
            stats: Arc::new(RwLock::new(CacheManagerStats {
                permission_cache_stats: CacheStatsSnapshot {
                    hits: 0,
                    misses: 0,
                    evictions: 0,
                    expired_evictions: 0,
                    size: 0,
                    capacity: 0,
                    hit_rate: 0.0,
                },
                abac_cache_stats: CacheStatsSnapshot {
                    hits: 0,
                    misses: 0,
                    evictions: 0,
                    expired_evictions: 0,
                    size: 0,
                    capacity: 0,
                    hit_rate: 0.0,
                },
                user_permissions_cache_stats: CacheStatsSnapshot {
                    hits: 0,
                    misses: 0,
                    evictions: 0,
                    expired_evictions: 0,
                    size: 0,
                    capacity: 0,
                    hit_rate: 0.0,
                },
                policy_cache_stats: CacheStatsSnapshot {
                    hits: 0,
                    misses: 0,
                    evictions: 0,
                    expired_evictions: 0,
                    size: 0,
                    capacity: 0,
                    hit_rate: 0.0,
                },
                resource_attributes_cache_stats: CacheStatsSnapshot {
                    hits: 0,
                    misses: 0,
                    evictions: 0,
                    expired_evictions: 0,
                    size: 0,
                    capacity: 0,
                    hit_rate: 0.0,
                },
                total_memory_usage_estimate: 0,
                cache_effectiveness: 0.0,
            })),
        }
    }

    /// Create cache manager from access control config
    pub fn from_access_control_config(config: &AccessControlConfig) -> Self {
        Self::new(CacheConfig::from(config))
    }

    // L1 Cache Methods (Evaluation Results)

    /// Get cached permission evaluation result
    pub async fn get_permission_result(&self, request: &RbacRequest) -> Option<RbacResult> {
        if !self.config.enabled {
            return None;
        }

        let key = PermissionCacheKey::from(request);
        let result = self.permission_cache.get(&key).await;

        if result.is_some() {
            debug!(
                "Permission cache hit for user {} on resource {}",
                request.user_id, request.resource
            );
        }

        result
    }

    /// Cache permission evaluation result
    pub async fn cache_permission_result(&self, request: &RbacRequest, result: &RbacResult) {
        if !self.config.enabled {
            return;
        }

        let key = PermissionCacheKey::from(request);
        let mut cached_result = result.clone();
        cached_result.cache_hit = true;

        self.permission_cache.insert(key, cached_result).await;
        debug!(
            "Cached permission result for user {} on resource {}",
            request.user_id, request.resource
        );
    }

    /// Get cached ABAC evaluation result
    pub async fn get_abac_result(&self, request: &AccessRequest) -> Option<AccessResponse> {
        if !self.config.enabled {
            return None;
        }

        let key = AbacCacheKey::from(request);
        let result = self.abac_cache.get(&key).await;

        if result.is_some() {
            debug!(
                "ABAC cache hit for subject {} on resource {}",
                request.subject.id, request.resource.id
            );
        }

        result
    }

    /// Cache ABAC evaluation result
    pub async fn cache_abac_result(&self, request: &AccessRequest, result: &AccessResponse) {
        if !self.config.enabled {
            return;
        }

        let key = AbacCacheKey::from(request);
        self.abac_cache.insert(key, result.clone()).await;
        debug!(
            "Cached ABAC result for subject {} on resource {}",
            request.subject.id, request.resource.id
        );
    }

    // L2 Cache Methods (User Data)

    /// Get cached user permissions
    pub async fn get_user_permissions(&self, user_id: i64) -> Option<Vec<Permission>> {
        if !self.config.enabled {
            return None;
        }

        let result = self.user_permissions_cache.get(&user_id).await;

        if result.is_some() {
            debug!("User permissions cache hit for user {}", user_id);
        }

        result
    }

    /// Cache user permissions
    pub async fn cache_user_permissions(&self, user_id: i64, permissions: &[Permission]) {
        if !self.config.enabled {
            return;
        }

        self.user_permissions_cache
            .insert(user_id, permissions.to_vec())
            .await;
        debug!("Cached permissions for user {}", user_id);
    }

    /// Get cached policy
    pub async fn get_policy(&self, policy_id: &str) -> Option<Policy> {
        if !self.config.enabled {
            return None;
        }

        let result = self.policy_cache.get(&policy_id.to_string()).await;

        if result.is_some() {
            debug!("Policy cache hit for policy {}", policy_id);
        }

        result
    }

    /// Cache policy
    pub async fn cache_policy(&self, policy: &Policy) {
        if !self.config.enabled {
            return;
        }

        self.policy_cache
            .insert(policy.id.clone(), policy.clone())
            .await;
        debug!("Cached policy {}", policy.id);
    }

    // L3 Cache Methods (Resource Data)

    /// Get cached resource attributes
    pub async fn get_resource_attributes(
        &self,
        resource_id: &str,
        resource_type: &str,
    ) -> Option<HashMap<String, serde_json::Value>> {
        if !self.config.enabled {
            return None;
        }

        let key = ResourceCacheKey {
            resource_id: resource_id.to_string(),
            resource_type: resource_type.to_string(),
        };

        let result = self.resource_attributes_cache.get(&key).await;

        if result.is_some() {
            debug!(
                "Resource attributes cache hit for resource {} of type {}",
                resource_id, resource_type
            );
        }

        result
    }

    /// Cache resource attributes
    pub async fn cache_resource_attributes(
        &self,
        resource_id: &str,
        resource_type: &str,
        attributes: &HashMap<String, serde_json::Value>,
    ) {
        if !self.config.enabled {
            return;
        }

        let key = ResourceCacheKey {
            resource_id: resource_id.to_string(),
            resource_type: resource_type.to_string(),
        };

        self.resource_attributes_cache
            .insert(key, attributes.clone())
            .await;
        debug!(
            "Cached attributes for resource {} of type {}",
            resource_id, resource_type
        );
    }

    // Cache Management Methods

    /// Invalidate all cache entries for a specific user
    pub async fn invalidate_user_cache(&self, user_id: i64) {
        // Remove user-specific entries from L1 caches
        self.permission_cache
            .remove_matching(|key| key.user_id == user_id)
            .await;
        self.abac_cache
            .remove_matching(|key| key.subject_id == user_id.to_string())
            .await;

        // Remove from L2 caches
        self.user_permissions_cache.remove(&user_id).await;

        info!("Invalidated all cache entries for user {}", user_id);
    }

    /// Invalidate cache entries for a specific policy
    pub async fn invalidate_policy_cache(&self, policy_id: &str) {
        // Remove policy from cache
        self.policy_cache.remove(&policy_id.to_string()).await;

        // Clear ABAC evaluation cache since policy changed
        self.abac_cache.clear().await;

        info!("Invalidated cache entries for policy {}", policy_id);
    }

    /// Invalidate cache entries for a specific resource
    pub async fn invalidate_resource_cache(&self, resource_id: &str, resource_type: Option<&str>) {
        if let Some(res_type) = resource_type {
            let key = ResourceCacheKey {
                resource_id: resource_id.to_string(),
                resource_type: res_type.to_string(),
            };
            self.resource_attributes_cache.remove(&key).await;
        } else {
            // Remove all entries for this resource regardless of type
            self.resource_attributes_cache
                .remove_matching(|key| key.resource_id == resource_id)
                .await;
        }

        // Clear related evaluation caches
        self.permission_cache
            .remove_matching(|key| key.resource == resource_id)
            .await;
        self.abac_cache
            .remove_matching(|key| key.resource_id == resource_id)
            .await;

        info!("Invalidated cache entries for resource {}", resource_id);
    }

    /// Clear all caches
    pub async fn clear_all_caches(&self) {
        self.permission_cache.clear().await;
        self.abac_cache.clear().await;
        self.user_permissions_cache.clear().await;
        self.policy_cache.clear().await;
        self.resource_attributes_cache.clear().await;

        info!("Cleared all caches");
    }

    /// Cleanup expired entries from all caches
    pub async fn cleanup_expired(&self) -> usize {
        let mut total_cleaned = 0;

        total_cleaned += self.permission_cache.cleanup_expired().await;
        total_cleaned += self.abac_cache.cleanup_expired().await;
        total_cleaned += self.user_permissions_cache.cleanup_expired().await;
        total_cleaned += self.policy_cache.cleanup_expired().await;
        total_cleaned += self.resource_attributes_cache.cleanup_expired().await;

        if total_cleaned > 0 {
            debug!("Cleaned up {} expired cache entries", total_cleaned);
        }

        total_cleaned
    }

    /// Get comprehensive cache statistics
    pub async fn get_stats(&self) -> CacheManagerStats {
        let permission_stats = self.permission_cache.stats().await;
        let abac_stats = self.abac_cache.stats().await;
        let user_permissions_stats = self.user_permissions_cache.stats().await;
        let policy_stats = self.policy_cache.stats().await;
        let resource_attributes_stats = self.resource_attributes_cache.stats().await;

        // Calculate overall cache effectiveness
        let total_hits = permission_stats.hits
            + abac_stats.hits
            + user_permissions_stats.hits
            + policy_stats.hits
            + resource_attributes_stats.hits;
        let total_requests = total_hits
            + permission_stats.misses
            + abac_stats.misses
            + user_permissions_stats.misses
            + policy_stats.misses
            + resource_attributes_stats.misses;

        let cache_effectiveness = if total_requests > 0 {
            total_hits as f64 / total_requests as f64
        } else {
            0.0
        };

        // Estimate memory usage (rough calculation)
        let total_memory_usage_estimate = (permission_stats.size * 1000) +  // Rough estimate per permission result
            (abac_stats.size * 2000) +        // ABAC results are larger
            (user_permissions_stats.size * 5000) + // User permissions array
            (policy_stats.size * 10000) +     // Policies are large
            (resource_attributes_stats.size * 2000); // Resource attributes

        CacheManagerStats {
            permission_cache_stats: permission_stats,
            abac_cache_stats: abac_stats,
            user_permissions_cache_stats: user_permissions_stats,
            policy_cache_stats: policy_stats,
            resource_attributes_cache_stats: resource_attributes_stats,
            total_memory_usage_estimate,
            cache_effectiveness,
        }
    }

    /// Check if caching is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Update cache configuration
    pub async fn update_config(&mut self, new_config: CacheConfig) {
        self.config = new_config;
        info!("Updated cache configuration");
    }

    /// Start background cleanup task
    pub async fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let cache_manager = self.clone();
        let cleanup_interval = self.config.cleanup_interval;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);

            loop {
                interval.tick().await;
                let cleaned = cache_manager.cleanup_expired().await;

                if cleaned > 0 {
                    debug!(
                        "Background cleanup removed {} expired cache entries",
                        cleaned
                    );
                }
            }
        })
    }
}

/// Implement the unified AccessControlCache trait for CacheManager
#[async_trait]
impl AccessControlCache for CacheManager {
    async fn get_permission_result(&self, request: &RbacRequest) -> Option<RbacResult> {
        self.get_permission_result(request).await
    }

    async fn cache_permission_result(&self, request: &RbacRequest, result: &RbacResult) {
        self.cache_permission_result(request, result).await
    }

    async fn get_user_permissions(&self, user_id: i64) -> Option<Vec<Permission>> {
        self.get_user_permissions(user_id).await
    }

    async fn cache_user_permissions(&self, user_id: i64, permissions: &[Permission]) {
        self.cache_user_permissions(user_id, permissions).await
    }

    async fn invalidate_user_cache(&self, user_id: i64) {
        self.invalidate_user_cache(user_id).await
    }

    async fn clear_all(&self) {
        self.clear_all_caches().await
    }

    async fn get_stats(&self) -> CacheStatsSnapshot {
        // Return aggregated stats from permission cache as primary metric
        let stats = self.get_stats().await;
        stats.permission_cache_stats
    }
}

// ============================================================================
// Cache Invalidation Manager
// ============================================================================

/// Cache invalidation strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvalidationStrategy {
    /// Immediate invalidation when changes occur
    Immediate,
    /// Batch invalidation at regular intervals
    Batched { interval: Duration },
    /// Time-based invalidation with TTL
    TimeBasedTtl { ttl: Duration },
    /// Hybrid approach combining immediate and batched
    Hybrid {
        immediate_threshold: usize,
        batch_interval: Duration,
    },
}

impl Default for InvalidationStrategy {
    fn default() -> Self {
        Self::Hybrid {
            immediate_threshold: 10,
            batch_interval: Duration::from_secs(30),
        }
    }
}

/// Invalidation configuration
#[derive(Debug, Clone)]
pub struct InvalidationConfig {
    /// Maximum number of events to batch before forcing immediate processing
    pub max_batch_size: usize,
    /// Maximum time to wait before processing batched events
    pub max_batch_delay: Duration,
    /// Enable/disable invalidation logging
    pub enable_logging: bool,
    /// Enable/disable invalidation metrics
    pub enable_metrics: bool,
}

impl Default for InvalidationConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            max_batch_delay: Duration::from_secs(60),
            enable_logging: true,
            enable_metrics: true,
        }
    }
}

/// Invalidation statistics
#[derive(Debug, Clone, Serialize)]
pub struct InvalidationStats {
    pub total_events_processed: u64,
    pub immediate_invalidations: u64,
    pub batched_invalidations: u64,
    pub failed_invalidations: u64,
    #[serde(with = "duration_serde")]
    pub average_processing_time: Duration,
    #[serde(skip)]
    pub last_invalidation_time: Option<Instant>,
    pub pending_events_count: usize,
}

mod duration_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    #[allow(dead_code)]
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

impl Default for InvalidationStats {
    fn default() -> Self {
        Self {
            total_events_processed: 0,
            immediate_invalidations: 0,
            batched_invalidations: 0,
            failed_invalidations: 0,
            average_processing_time: Duration::from_millis(0),
            last_invalidation_time: None,
            pending_events_count: 0,
        }
    }
}

/// Cache invalidation manager
#[derive(Clone)]
pub struct CacheInvalidationManager {
    cache_manager: Arc<CacheManager>,
    strategy: InvalidationStrategy,
    pending_events: Arc<RwLock<HashSet<InvalidationEvent>>>,
    batch_processor: Arc<Notify>,
    stats: Arc<RwLock<InvalidationStats>>,
    config: InvalidationConfig,
}

impl CacheInvalidationManager {
    /// Create a new cache invalidation manager
    pub fn new(
        cache_manager: CacheManager,
        strategy: InvalidationStrategy,
        config: InvalidationConfig,
    ) -> Self {
        Self {
            cache_manager: Arc::new(cache_manager),
            strategy,
            pending_events: Arc::new(RwLock::new(HashSet::new())),
            batch_processor: Arc::new(Notify::new()),
            stats: Arc::new(RwLock::new(InvalidationStats::default())),
            config,
        }
    }

    /// Create from access control config
    pub fn from_access_control_config(
        cache_manager: CacheManager,
        access_config: &AccessControlConfig,
    ) -> Self {
        let strategy = if access_config.enable_caching {
            InvalidationStrategy::Hybrid {
                immediate_threshold: 5,
                batch_interval: Duration::from_secs(access_config.cache_ttl_seconds / 10),
            }
        } else {
            InvalidationStrategy::Immediate
        };

        Self::new(cache_manager, strategy, InvalidationConfig::default())
    }

    /// Process an invalidation event
    pub async fn invalidate(&self, event: InvalidationEvent) -> AccessControlResult<()> {
        let start_time = Instant::now();

        match &self.strategy {
            InvalidationStrategy::Immediate => {
                if let Err(e) = self.process_event_immediate(&event).await {
                    warn!("Immediate invalidation failed for event {:?}: {}", event, e);
                    self.increment_failed_stats().await;
                } else {
                    self.update_stats_immediate(start_time).await;
                }
            }
            InvalidationStrategy::Batched { .. } => {
                self.add_to_batch(event).await;
            }
            InvalidationStrategy::TimeBasedTtl { .. } => {
                debug!("TTL-based invalidation - no immediate action needed");
            }
            InvalidationStrategy::Hybrid {
                immediate_threshold,
                ..
            } => {
                let pending_count = {
                    let pending = self.pending_events.read().await;
                    pending.len()
                };

                if pending_count + 1 >= *immediate_threshold {
                    if let Err(e) = self.process_event_immediate(&event).await {
                        warn!(
                            "Hybrid immediate invalidation failed for event {:?}: {}",
                            event, e
                        );
                        self.increment_failed_stats().await;
                    } else {
                        self.update_stats_immediate(start_time).await;
                    }
                } else {
                    self.add_to_batch(event).await;
                }
            }
        }

        Ok(())
    }

    /// Process multiple invalidation events
    pub async fn invalidate_batch(
        &self,
        events: Vec<InvalidationEvent>,
    ) -> AccessControlResult<()> {
        let start_time = Instant::now();

        for event in events {
            if let Err(e) = self.process_event_immediate(&event).await {
                warn!("Failed to process invalidation event {:?}: {}", event, e);
                self.increment_failed_stats().await;
            }
        }

        self.update_stats_batched(start_time).await;
        Ok(())
    }

    /// Process an event immediately
    async fn process_event_immediate(&self, event: &InvalidationEvent) -> AccessControlResult<()> {
        if self.config.enable_logging {
            debug!(
                "Processing invalidation event: {} (scope: {:?})",
                event.description(),
                event.scope()
            );
        }

        if event.is_critical() {
            warn!(
                "Processing critical invalidation event: {}",
                event.description()
            );
        }

        match event {
            InvalidationEvent::UserUpdated { user_id }
            | InvalidationEvent::UserDeleted { user_id }
            | InvalidationEvent::UserPermissionsChanged { user_id }
            | InvalidationEvent::UserAttributesChanged { user_id } => {
                self.invalidate_user_specific_cache(*user_id).await?;
                info!("Invalidated cache for user {}", user_id);
            }

            InvalidationEvent::RoleCreated { role_id } => {
                debug!("Role {} created - no cache invalidation needed", role_id);
            }

            InvalidationEvent::RoleUpdated { role_id }
            | InvalidationEvent::RoleDeleted { role_id }
            | InvalidationEvent::RolePermissionsChanged { role_id } => {
                self.invalidate_role_related_cache(role_id).await?;
                info!("Invalidated cache for role {}", role_id);
            }

            InvalidationEvent::PolicyCreated { policy_id } => {
                debug!(
                    "Policy {} created - no cache invalidation needed",
                    policy_id
                );
            }

            InvalidationEvent::PolicyUpdated { policy_id }
            | InvalidationEvent::PolicyDeleted { policy_id } => {
                self.invalidate_policy_related_cache(policy_id).await?;
                info!("Invalidated cache for policy {}", policy_id);
            }

            InvalidationEvent::PolicyActivated { policy_id }
            | InvalidationEvent::PolicyDeactivated { policy_id } => {
                self.invalidate_policy_related_cache(policy_id).await?;
                warn!(
                    "Policy {} activation changed - cleared ABAC evaluation cache",
                    policy_id
                );
            }

            InvalidationEvent::ResourceCreated {
                resource_id,
                resource_type,
            } => {
                debug!(
                    "Resource {} of type {} created - no cache invalidation needed",
                    resource_id, resource_type
                );
            }

            InvalidationEvent::ResourceUpdated {
                resource_id,
                resource_type,
            }
            | InvalidationEvent::ResourceDeleted {
                resource_id,
                resource_type,
            }
            | InvalidationEvent::ResourceAttributesChanged {
                resource_id,
                resource_type,
            } => {
                self.invalidate_resource_specific_cache(resource_id, resource_type)
                    .await?;
                info!(
                    "Invalidated cache for resource {} of type {}",
                    resource_id, resource_type
                );
            }

            InvalidationEvent::PermissionCreated { permission_id } => {
                debug!(
                    "Permission {} created - no cache invalidation needed",
                    permission_id
                );
            }

            InvalidationEvent::PermissionUpdated { permission_id }
            | InvalidationEvent::PermissionDeleted { permission_id } => {
                self.invalidate_permission_related_cache(permission_id)
                    .await?;
                warn!(
                    "Permission {} changed - cleared all permission evaluation caches",
                    permission_id
                );
            }

            InvalidationEvent::ConfigurationChanged
            | InvalidationEvent::SystemMaintenance
            | InvalidationEvent::ClearAll => {
                self.cache_manager.clear_all_caches().await;
                warn!("System-wide cache clear: {}", event.description());
            }
        }

        Ok(())
    }

    async fn invalidate_user_specific_cache(&self, user_id: i64) -> AccessControlResult<()> {
        self.cache_manager.invalidate_user_cache(user_id).await;
        Ok(())
    }

    async fn invalidate_role_related_cache(&self, _role_id: &str) -> AccessControlResult<()> {
        self.cache_manager.clear_all_caches().await;
        Ok(())
    }

    async fn invalidate_policy_related_cache(&self, policy_id: &str) -> AccessControlResult<()> {
        self.cache_manager.invalidate_policy_cache(policy_id).await;
        Ok(())
    }

    async fn invalidate_resource_specific_cache(
        &self,
        resource_id: &str,
        resource_type: &str,
    ) -> AccessControlResult<()> {
        self.cache_manager
            .invalidate_resource_cache(resource_id, Some(resource_type))
            .await;
        Ok(())
    }

    async fn invalidate_permission_related_cache(
        &self,
        _permission_id: &str,
    ) -> AccessControlResult<()> {
        self.cache_manager.clear_all_caches().await;
        Ok(())
    }

    async fn add_to_batch(&self, event: InvalidationEvent) {
        let mut pending = self.pending_events.write().await;
        pending.insert(event);

        if pending.len() >= self.config.max_batch_size {
            drop(pending);
            self.batch_processor.notify_one();
        }
    }

    async fn process_pending_batch(&self) -> AccessControlResult<()> {
        let events = {
            let mut pending = self.pending_events.write().await;
            let events: Vec<_> = pending.drain().collect();
            events
        };

        if !events.is_empty() {
            if self.config.enable_logging {
                info!("Processing batch of {} invalidation events", events.len());
            }

            self.invalidate_batch(events).await?;
        }

        Ok(())
    }

    pub async fn start_batch_processor(&self) -> tokio::task::JoinHandle<()> {
        let manager = self.clone();
        let batch_interval = match &self.strategy {
            InvalidationStrategy::Batched { interval } => *interval,
            InvalidationStrategy::Hybrid { batch_interval, .. } => *batch_interval,
            _ => Duration::from_secs(30),
        };

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(batch_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = manager.process_pending_batch().await {
                            error!("Failed to process pending batch: {}", e);
                        }
                    }
                    _ = manager.batch_processor.notified() => {
                        if let Err(e) = manager.process_pending_batch().await {
                            error!("Failed to process forced batch: {}", e);
                        }
                    }
                }
            }
        })
    }

    async fn update_stats_immediate(&self, start_time: Instant) {
        let mut stats = self.stats.write().await;
        stats.total_events_processed += 1;
        stats.immediate_invalidations += 1;
        stats.last_invalidation_time = Some(Instant::now());

        let processing_time = start_time.elapsed();
        stats.average_processing_time = Duration::from_nanos(
            ((stats.average_processing_time.as_nanos() as u64
                * (stats.total_events_processed - 1))
                + processing_time.as_nanos() as u64)
                / stats.total_events_processed,
        );
    }

    async fn update_stats_batched(&self, start_time: Instant) {
        let mut stats = self.stats.write().await;
        stats.batched_invalidations += 1;
        stats.last_invalidation_time = Some(Instant::now());

        let processing_time = start_time.elapsed();
        if stats.total_events_processed > 0 {
            stats.average_processing_time = Duration::from_nanos(
                ((stats.average_processing_time.as_nanos() as u64 * stats.total_events_processed)
                    + processing_time.as_nanos() as u64)
                    / (stats.total_events_processed + 1),
            );
        } else {
            stats.average_processing_time = processing_time;
        }
    }

    async fn increment_failed_stats(&self) {
        let mut stats = self.stats.write().await;
        stats.failed_invalidations += 1;
    }

    pub async fn get_stats(&self) -> InvalidationStats {
        let mut stats = self.stats.read().await.clone();
        let pending = self.pending_events.read().await;
        stats.pending_events_count = pending.len();
        stats
    }

    pub async fn update_strategy(&mut self, new_strategy: InvalidationStrategy) {
        if let Err(e) = self.process_pending_batch().await {
            warn!(
                "Failed to process pending batch before strategy change: {}",
                e
            );
        }

        self.strategy = new_strategy;
        info!("Updated cache invalidation strategy to {:?}", self.strategy);
    }

    pub async fn force_process_pending(&self) -> AccessControlResult<()> {
        self.process_pending_batch().await
    }

    pub async fn clear_pending(&self) {
        let mut pending = self.pending_events.write().await;
        let count = pending.len();
        pending.clear();

        if count > 0 {
            warn!(
                "Cleared {} pending invalidation events without processing",
                count
            );
        }
    }

    // Convenience methods
    pub async fn on_user_permissions_changed(&self, user_id: i64) -> AccessControlResult<()> {
        self.invalidate(InvalidationEvent::UserPermissionsChanged { user_id })
            .await
    }

    pub async fn on_user_attributes_changed(&self, user_id: i64) -> AccessControlResult<()> {
        self.invalidate(InvalidationEvent::UserAttributesChanged { user_id })
            .await
    }

    pub async fn on_policy_updated(&self, policy_id: String) -> AccessControlResult<()> {
        self.invalidate(InvalidationEvent::PolicyUpdated { policy_id })
            .await
    }

    pub async fn on_resource_attributes_changed(
        &self,
        resource_id: String,
        resource_type: String,
    ) -> AccessControlResult<()> {
        self.invalidate(InvalidationEvent::ResourceAttributesChanged {
            resource_id,
            resource_type,
        })
        .await
    }

    pub async fn on_configuration_changed(&self) -> AccessControlResult<()> {
        self.invalidate(InvalidationEvent::ConfigurationChanged)
            .await
    }

    pub async fn clear_all(&self) -> AccessControlResult<()> {
        self.invalidate(InvalidationEvent::ClearAll).await
    }

    pub async fn invalidate_with_retry(
        &self,
        event: InvalidationEvent,
        max_retries: u32,
    ) -> AccessControlResult<()> {
        let mut attempts = 0;
        let mut last_error = None;

        while attempts < max_retries {
            match self.process_event_immediate(&event).await {
                Ok(_) => {
                    if attempts > 0 {
                        info!(
                            "Cache invalidation succeeded after {} retries for event: {}",
                            attempts,
                            event.description()
                        );
                    }
                    return Ok(());
                }
                Err(e) => {
                    attempts += 1;
                    last_error = Some(e);

                    if attempts < max_retries {
                        let delay = Duration::from_millis(100 * 2u64.pow(attempts - 1));
                        warn!(
                            "Cache invalidation attempt {} failed for event {}, retrying in {:?}",
                            attempts,
                            event.description(),
                            delay
                        );
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        if let Some(e) = last_error {
            error!(
                "Cache invalidation failed after {} attempts for event {}: {}",
                max_retries,
                event.description(),
                e
            );
            self.increment_failed_stats().await;
        }

        Ok(())
    }

    pub async fn get_failed_count(&self) -> u64 {
        let stats = self.stats.read().await;
        stats.failed_invalidations
    }

    pub async fn reset_failed_count(&self) {
        let mut stats = self.stats.write().await;
        stats.failed_invalidations = 0;
        info!("Reset failed invalidation count");
    }
}

// ============================================================================
// Background Task Manager
// ============================================================================

/// Configuration for background tasks
#[derive(Debug, Clone)]
pub struct BackgroundTaskConfig {
    /// Interval for cleanup tasks
    pub cleanup_interval: Duration,
    /// Interval for statistics collection
    pub stats_interval: Duration,
    /// Enable/disable background tasks
    pub enabled: bool,
    /// Maximum cleanup batch size
    pub max_cleanup_batch_size: usize,
}

impl Default for BackgroundTaskConfig {
    fn default() -> Self {
        Self {
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            stats_interval: Duration::from_secs(60),    // 1 minute
            enabled: true,
            max_cleanup_batch_size: 1000,
        }
    }
}

/// Statistics for background tasks
#[derive(Debug, Clone)]
pub struct BackgroundTaskStats {
    pub cleanup_runs: u64,
    pub total_cleaned_entries: u64,
    pub last_cleanup_time: Option<Instant>,
    pub last_cleanup_duration: Option<Duration>,
    pub stats_collections: u64,
    pub last_stats_collection_time: Option<Instant>,
    pub task_errors: u64,
}

impl Default for BackgroundTaskStats {
    fn default() -> Self {
        Self {
            cleanup_runs: 0,
            total_cleaned_entries: 0,
            last_cleanup_time: None,
            last_cleanup_duration: None,
            stats_collections: 0,
            last_stats_collection_time: None,
            task_errors: 0,
        }
    }
}

/// Background task manager for cache operations
#[derive(Clone)]
pub struct CacheBackgroundTaskManager {
    cache_manager: Arc<CacheManager>,
    invalidation_manager: Option<Arc<CacheInvalidationManager>>,
    config: BackgroundTaskConfig,
}

impl CacheBackgroundTaskManager {
    /// Create a new background task manager
    pub fn new(
        cache_manager: CacheManager,
        invalidation_manager: Option<CacheInvalidationManager>,
        config: BackgroundTaskConfig,
    ) -> Self {
        Self {
            cache_manager: Arc::new(cache_manager),
            invalidation_manager: invalidation_manager.map(Arc::new),
            config,
        }
    }

    /// Start all background tasks
    pub async fn start_all_tasks(&self) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        if !self.config.enabled {
            info!("Background tasks are disabled");
            return handles;
        }

        handles.push(self.start_cleanup_task().await);
        handles.push(self.start_stats_collection_task().await);

        if let Some(invalidation_manager) = &self.invalidation_manager {
            handles.push(invalidation_manager.start_batch_processor().await);
        }

        info!(
            "Started {} background tasks for cache management",
            handles.len()
        );
        handles
    }

    /// Start the cache cleanup task
    pub async fn start_cleanup_task(&self) -> JoinHandle<()> {
        let cache_manager = Arc::clone(&self.cache_manager);
        let cleanup_interval = self.config.cleanup_interval;
        let max_batch_size = self.config.max_cleanup_batch_size;

        tokio::spawn(async move {
            let mut cleanup_interval_timer = interval(cleanup_interval);
            let mut stats = BackgroundTaskStats::default();

            info!(
                "Started cache cleanup task with interval {:?}",
                cleanup_interval
            );

            loop {
                cleanup_interval_timer.tick().await;

                let start_time = Instant::now();
                match Self::perform_cleanup(&cache_manager, max_batch_size).await {
                    Ok(cleaned_count) => {
                        stats.cleanup_runs += 1;
                        stats.total_cleaned_entries += cleaned_count as u64;
                        stats.last_cleanup_time = Some(start_time);
                        stats.last_cleanup_duration = Some(start_time.elapsed());

                        if cleaned_count > 0 {
                            debug!(
                                "Cache cleanup completed: {} entries cleaned in {:?}",
                                cleaned_count,
                                start_time.elapsed()
                            );
                        }
                    }
                    Err(e) => {
                        stats.task_errors += 1;
                        error!("Cache cleanup failed: {}", e);
                    }
                }
            }
        })
    }

    /// Start the statistics collection task
    pub async fn start_stats_collection_task(&self) -> JoinHandle<()> {
        let cache_manager = Arc::clone(&self.cache_manager);
        let stats_interval = self.config.stats_interval;

        tokio::spawn(async move {
            let mut stats_interval_timer = interval(stats_interval);
            let mut stats = BackgroundTaskStats::default();

            debug!(
                "Started cache statistics collection task with interval {:?}",
                stats_interval
            );

            loop {
                stats_interval_timer.tick().await;

                match Self::collect_and_log_stats(&cache_manager).await {
                    Ok(_) => {
                        stats.stats_collections += 1;
                        stats.last_stats_collection_time = Some(Instant::now());
                    }
                    Err(e) => {
                        stats.task_errors += 1;
                        warn!("Statistics collection failed: {}", e);
                    }
                }
            }
        })
    }

    async fn perform_cleanup(
        cache_manager: &CacheManager,
        max_batch_size: usize,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let mut total_cleaned = 0;
        let mut batch_count = 0;

        loop {
            let cleaned = cache_manager.cleanup_expired().await;
            total_cleaned += cleaned;
            batch_count += 1;

            if cleaned == 0 || batch_count >= max_batch_size {
                break;
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Ok(total_cleaned)
    }

    async fn collect_and_log_stats(
        cache_manager: &CacheManager,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stats = cache_manager.get_stats().await;

        debug!(
            "Cache Stats - Effectiveness: {:.2}%, Memory: {}KB, L1 Hit Rate: {:.2}%, L2 Hit Rate: {:.2}%",
            stats.cache_effectiveness * 100.0,
            stats.total_memory_usage_estimate / 1024,
            stats.permission_cache_stats.hit_rate * 100.0,
            stats.user_permissions_cache_stats.hit_rate * 100.0
        );

        if stats.cache_effectiveness < 0.5 {
            warn!(
                "Cache effectiveness is low: {:.2}% - consider reviewing cache configuration",
                stats.cache_effectiveness * 100.0
            );
        }

        if stats.permission_cache_stats.hit_rate < 0.3 {
            warn!(
                "Permission cache hit rate is low: {:.2}% - consider increasing TTL or capacity",
                stats.permission_cache_stats.hit_rate * 100.0
            );
        }

        Ok(())
    }

    pub async fn start_custom_task<F, Fut>(
        &self,
        name: &str,
        task_interval: Duration,
        task: F,
    ) -> JoinHandle<()>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    {
        let task_name = name.to_string();

        tokio::spawn(async move {
            let mut custom_task_interval = interval(task_interval);
            let mut error_count = 0;

            info!("Started custom cache task: {}", task_name);

            loop {
                custom_task_interval.tick().await;

                match task().await {
                    Ok(_) => {
                        error_count = 0;
                    }
                    Err(e) => {
                        error_count += 1;
                        error!(
                            "Custom cache task '{}' failed (error #{}) : {}",
                            task_name, error_count, e
                        );

                        if error_count > 3 {
                            let backoff_duration =
                                Duration::from_secs(2_u64.pow((error_count - 3).min(6)));
                            warn!(
                                "Task '{}' backing off for {:?} due to repeated failures",
                                task_name, backoff_duration
                            );
                            tokio::time::sleep(backoff_duration).await;
                        }
                    }
                }
            }
        })
    }

    pub async fn stop_all_tasks(handles: Vec<JoinHandle<()>>) {
        info!("Stopping {} background cache tasks", handles.len());

        for handle in handles {
            handle.abort();
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
        info!("All background cache tasks stopped");
    }

    pub fn cache_manager(&self) -> &Arc<CacheManager> {
        &self.cache_manager
    }

    pub fn invalidation_manager(&self) -> Option<&Arc<CacheInvalidationManager>> {
        self.invalidation_manager.as_ref()
    }

    pub fn update_config(&mut self, new_config: BackgroundTaskConfig) {
        self.config = new_config;
        info!("Updated background task configuration");
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub async fn perform_immediate_maintenance(
        &self,
    ) -> Result<MaintenanceResult, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();

        let cleaned_entries = self.cache_manager.cleanup_expired().await;

        if let Some(invalidation_manager) = &self.invalidation_manager {
            invalidation_manager.force_process_pending().await?;
        }

        let stats = self.cache_manager.get_stats().await;

        Ok(MaintenanceResult {
            cleaned_entries,
            maintenance_duration: start_time.elapsed(),
            cache_stats: stats,
        })
    }

    pub async fn warm_up_cache(
        &self,
        warm_up_requests: Vec<CacheWarmUpRequest>,
    ) -> Result<WarmUpResult, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let mut successful_warm_ups = 0;
        let mut failed_warm_ups = 0;

        for request in warm_up_requests {
            match self.process_warm_up_request(request).await {
                Ok(_) => successful_warm_ups += 1,
                Err(e) => {
                    failed_warm_ups += 1;
                    warn!("Cache warm-up request failed: {}", e);
                }
            }
        }

        Ok(WarmUpResult {
            successful_warm_ups,
            failed_warm_ups,
            warm_up_duration: start_time.elapsed(),
        })
    }

    async fn process_warm_up_request(
        &self,
        request: CacheWarmUpRequest,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match request {
            CacheWarmUpRequest::UserPermissions { user_id } => {
                debug!("Warming up cache for user permissions: {}", user_id);
                Ok(())
            }
            CacheWarmUpRequest::Policy { policy_id } => {
                debug!("Warming up cache for policy: {}", policy_id);
                Ok(())
            }
            CacheWarmUpRequest::ResourceAttributes {
                resource_id,
                resource_type,
            } => {
                debug!(
                    "Warming up cache for resource: {} of type {}",
                    resource_id, resource_type
                );
                Ok(())
            }
        }
    }
}

/// Result of maintenance operation
#[derive(Debug, Clone)]
pub struct MaintenanceResult {
    pub cleaned_entries: usize,
    pub maintenance_duration: Duration,
    pub cache_stats: CacheManagerStats,
}

/// Result of cache warm-up operation
#[derive(Debug, Clone)]
pub struct WarmUpResult {
    pub successful_warm_ups: usize,
    pub failed_warm_ups: usize,
    pub warm_up_duration: Duration,
}

/// Cache warm-up request types
#[derive(Debug, Clone)]
pub enum CacheWarmUpRequest {
    UserPermissions {
        user_id: i64,
    },
    Policy {
        policy_id: String,
    },
    ResourceAttributes {
        resource_id: String,
        resource_type: String,
    },
}
