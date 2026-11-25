use crate::services::policy::update_manager::{
    AuditLoggingListener, CacheInvalidationListener, PolicyUpdateListener, PolicyUpdateManager,
};
use std::sync::Arc;

/// Initialize default policy update listeners
pub async fn initialize_policy_listeners(
    policy_update_manager: Arc<PolicyUpdateManager>,
) -> Result<(), String> {
    tracing::info!("Initializing policy update listeners...");

    // Register audit logging listener
    let audit_listener = Arc::new(AuditLoggingListener);
    policy_update_manager
        .register_listener("audit_logger".to_string(), audit_listener)
        .await?;

    // Register cache invalidation listeners for different cache types
    let rbac_cache_listener = Arc::new(CacheInvalidationListener::new("rbac_cache".to_string()));
    policy_update_manager
        .register_listener("rbac_cache_invalidator".to_string(), rbac_cache_listener)
        .await?;

    let abac_cache_listener = Arc::new(CacheInvalidationListener::new("abac_cache".to_string()));
    policy_update_manager
        .register_listener("abac_cache_invalidator".to_string(), abac_cache_listener)
        .await?;

    let permission_cache_listener = Arc::new(CacheInvalidationListener::new(
        "permission_cache".to_string(),
    ));
    policy_update_manager
        .register_listener(
            "permission_cache_invalidator".to_string(),
            permission_cache_listener,
        )
        .await?;

    tracing::info!("Policy update listeners initialized successfully");
    Ok(())
}

/// Custom listener for database synchronization
pub struct DatabaseSyncListener {
    // Database synchronization is handled by the PolicyRepository
    // This listener logs sync events for audit purposes
}

impl DatabaseSyncListener {
    pub fn new() -> Self {
        Self {}
    }
}

impl PolicyUpdateListener for DatabaseSyncListener {
    fn handle_update(
        &self,
        event: &crate::services::policy::update_manager::PolicyUpdateEvent,
    ) -> Result<(), String> {
        use crate::services::policy::update_manager::PolicyUpdateEvent;

        match event {
            PolicyUpdateEvent::PolicyCreated { policy_id, policy_name, timestamp } => {
                tracing::info!(
                    "Database sync: Policy created - ID: {}, Name: {}, Time: {}",
                    policy_id, policy_name, timestamp
                );
                // Database writes are handled by PolicyRepository during policy creation
                // This listener provides audit logging for synchronization events
            }
            PolicyUpdateEvent::PolicyUpdated { policy_id, policy_name, timestamp } => {
                tracing::info!(
                    "Database sync: Policy updated - ID: {}, Name: {}, Time: {}",
                    policy_id, policy_name, timestamp
                );
                // Database updates are handled by PolicyRepository during policy updates
                // This listener provides audit logging for synchronization events
            }
            PolicyUpdateEvent::PolicyDeleted { policy_id, policy_name, timestamp } => {
                tracing::info!(
                    "Database sync: Policy deleted - ID: {}, Name: {}, Time: {}",
                    policy_id, policy_name, timestamp
                );
                // Database deletions are handled by PolicyRepository during policy deletion
                // This listener provides audit logging for synchronization events
            }
            PolicyUpdateEvent::PolicyActivated { policy_id, policy_name, timestamp } => {
                tracing::info!(
                    "Database sync: Policy activated - ID: {}, Name: {}, Time: {}",
                    policy_id, policy_name, timestamp
                );
                // Policy activation is handled by PolicyRepository during policy updates
                // This listener provides audit logging for synchronization events
            }
            PolicyUpdateEvent::PolicyDeactivated { policy_id, policy_name, timestamp } => {
                tracing::info!(
                    "Database sync: Policy deactivated - ID: {}, Name: {}, Time: {}",
                    policy_id, policy_name, timestamp
                );
                // Policy deactivation is handled by PolicyRepository during policy updates
                // This listener provides audit logging for synchronization events
            }
            PolicyUpdateEvent::CacheInvalidation { .. } => {
                // No database sync needed for cache invalidation
            }
        }
        Ok(())
    }
}

/// Custom listener for metrics collection
pub struct MetricsCollectionListener {
    // Metrics are collected using atomic counters for thread-safe updates
    policy_created_count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    policy_updated_count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    policy_deleted_count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    policy_activated_count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    policy_deactivated_count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    cache_invalidation_count: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl MetricsCollectionListener {
    pub fn new() -> Self {
        Self {
            policy_created_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            policy_updated_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            policy_deleted_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            policy_activated_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            policy_deactivated_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            cache_invalidation_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Get the count of policy creation events
    pub fn get_created_count(&self) -> u64 {
        self.policy_created_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the count of policy update events
    pub fn get_updated_count(&self) -> u64 {
        self.policy_updated_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the count of policy deletion events
    pub fn get_deleted_count(&self) -> u64 {
        self.policy_deleted_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the count of policy activation events
    pub fn get_activated_count(&self) -> u64 {
        self.policy_activated_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the count of policy deactivation events
    pub fn get_deactivated_count(&self) -> u64 {
        self.policy_deactivated_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the count of cache invalidation events
    pub fn get_cache_invalidation_count(&self) -> u64 {
        self.cache_invalidation_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Reset all metrics counters
    pub fn reset_metrics(&self) {
        self.policy_created_count.store(0, std::sync::atomic::Ordering::Relaxed);
        self.policy_updated_count.store(0, std::sync::atomic::Ordering::Relaxed);
        self.policy_deleted_count.store(0, std::sync::atomic::Ordering::Relaxed);
        self.policy_activated_count.store(0, std::sync::atomic::Ordering::Relaxed);
        self.policy_deactivated_count.store(0, std::sync::atomic::Ordering::Relaxed);
        self.cache_invalidation_count.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

impl PolicyUpdateListener for MetricsCollectionListener {
    fn handle_update(
        &self,
        event: &crate::services::policy::update_manager::PolicyUpdateEvent,
    ) -> Result<(), String> {
        use crate::services::policy::update_manager::PolicyUpdateEvent;

        match event {
            PolicyUpdateEvent::PolicyCreated { .. } => {
                self.policy_created_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            PolicyUpdateEvent::PolicyUpdated { .. } => {
                self.policy_updated_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            PolicyUpdateEvent::PolicyDeleted { .. } => {
                self.policy_deleted_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            PolicyUpdateEvent::PolicyActivated { .. } => {
                self.policy_activated_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            PolicyUpdateEvent::PolicyDeactivated { .. } => {
                self.policy_deactivated_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            PolicyUpdateEvent::CacheInvalidation { .. } => {
                self.cache_invalidation_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        Ok(())
    }
}

/// Initialize advanced policy update listeners
pub async fn initialize_advanced_listeners(
    policy_update_manager: Arc<PolicyUpdateManager>,
) -> Result<(), String> {
    tracing::info!("Initializing advanced policy update listeners...");

    // Register database sync listener
    let db_sync_listener = Arc::new(DatabaseSyncListener::new());
    policy_update_manager
        .register_listener("database_sync".to_string(), db_sync_listener)
        .await?;

    // Register metrics collection listener
    let metrics_listener = Arc::new(MetricsCollectionListener::new());
    policy_update_manager
        .register_listener("metrics_collector".to_string(), metrics_listener)
        .await?;

    tracing::info!("Advanced policy update listeners initialized successfully");
    Ok(())
}

/// Start background task for policy update processing
pub async fn start_policy_update_background_task(
    policy_update_manager: Arc<PolicyUpdateManager>,
) -> Result<(), String> {
    let mut receiver = policy_update_manager.subscribe();

    tokio::spawn(async move {
        tracing::info!("Policy update background task started");

        while let Ok(event) = receiver.recv().await {
            tracing::debug!("Background task received policy update event: {:?}", event);

            // Additional background processing can be added here
            // For example:
            // - Sending notifications to external systems
            // - Triggering webhooks
            // - Updating external caches
            // - Generating reports
        }

        tracing::warn!("Policy update background task ended");
    });

    Ok(())
}
