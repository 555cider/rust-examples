use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Policy update event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyUpdateEvent {
    /// A policy was created
    PolicyCreated {
        policy_id: String,
        policy_name: String,
        timestamp: DateTime<Utc>,
    },
    /// A policy was updated
    PolicyUpdated {
        policy_id: String,
        policy_name: String,
        timestamp: DateTime<Utc>,
    },
    /// A policy was deleted
    PolicyDeleted {
        policy_id: String,
        policy_name: String,
        timestamp: DateTime<Utc>,
    },
    /// A policy was activated
    PolicyActivated {
        policy_id: String,
        policy_name: String,
        timestamp: DateTime<Utc>,
    },
    /// A policy was deactivated
    PolicyDeactivated {
        policy_id: String,
        policy_name: String,
        timestamp: DateTime<Utc>,
    },
    /// Cache invalidation request
    CacheInvalidation {
        policy_ids: Vec<String>,
        timestamp: DateTime<Utc>,
    },
}

/// Policy update listener trait
pub trait PolicyUpdateListener: Send + Sync {
    /// Handle a policy update event
    fn handle_update(&self, event: &PolicyUpdateEvent) -> Result<(), String>;
}

/// Policy update manager for real-time policy updates
pub struct PolicyUpdateManager {
    /// Broadcast channel for policy updates
    sender: broadcast::Sender<PolicyUpdateEvent>,
    /// Registered listeners
    listeners: Arc<RwLock<HashMap<String, Arc<dyn PolicyUpdateListener>>>>,
    /// Update history for debugging
    update_history: Arc<RwLock<Vec<PolicyUpdateEvent>>>,
    /// Maximum history size
    max_history_size: usize,
}

impl PolicyUpdateManager {
    /// Create a new policy update manager
    pub fn new(channel_capacity: usize, max_history_size: usize) -> Self {
        let (sender, _) = broadcast::channel(channel_capacity);

        Self {
            sender,
            listeners: Arc::new(RwLock::new(HashMap::new())),
            update_history: Arc::new(RwLock::new(Vec::new())),
            max_history_size,
        }
    }

    /// Register a policy update listener
    pub async fn register_listener(
        &self,
        listener_id: String,
        listener: Arc<dyn PolicyUpdateListener>,
    ) -> Result<(), String> {
        let mut listeners = self.listeners.write().await;
        listeners.insert(listener_id.clone(), listener);
        tracing::info!("Registered policy update listener: {}", listener_id);
        Ok(())
    }

    /// Unregister a policy update listener
    pub async fn unregister_listener(&self, listener_id: &str) -> Result<(), String> {
        let mut listeners = self.listeners.write().await;
        if listeners.remove(listener_id).is_some() {
            tracing::info!("Unregistered policy update listener: {}", listener_id);
            Ok(())
        } else {
            Err(format!("Listener not found: {}", listener_id))
        }
    }

    /// Publish a policy update event
    pub async fn publish_update(&self, event: PolicyUpdateEvent) -> Result<(), String> {
        // Add to history
        {
            let mut history = self.update_history.write().await;
            history.push(event.clone());

            // Trim history if it exceeds max size
            if history.len() > self.max_history_size {
                let excess = history.len() - self.max_history_size;
                history.drain(0..excess);
            }
        }

        // Notify registered listeners
        {
            let listeners = self.listeners.read().await;
            for (listener_id, listener) in listeners.iter() {
                if let Err(e) = listener.handle_update(&event) {
                    tracing::warn!("Listener {} failed to handle update: {}", listener_id, e);
                }
            }
        }

        // Broadcast to subscribers (ignore error if no subscribers)
        let _ = self.sender.send(event.clone());

        Ok(())
    }

    /// Subscribe to policy updates
    pub fn subscribe(&self) -> broadcast::Receiver<PolicyUpdateEvent> {
        self.sender.subscribe()
    }

    /// Get update history
    pub async fn get_update_history(&self) -> Vec<PolicyUpdateEvent> {
        self.update_history.read().await.clone()
    }

    /// Get recent updates (last N events)
    pub async fn get_recent_updates(&self, limit: usize) -> Vec<PolicyUpdateEvent> {
        let history = self.update_history.read().await;
        let start_index = if history.len() > limit {
            history.len() - limit
        } else {
            0
        };
        history[start_index..].to_vec()
    }

    /// Clear update history
    pub async fn clear_history(&self) -> Result<(), String> {
        let mut history = self.update_history.write().await;
        history.clear();
        tracing::info!("Policy update history cleared");
        Ok(())
    }

    /// Get listener count
    pub async fn get_listener_count(&self) -> usize {
        self.listeners.read().await.len()
    }

    /// Get subscriber count
    pub fn get_subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Publish policy created event
    pub async fn notify_policy_created(
        &self,
        policy_id: String,
        policy_name: String,
    ) -> Result<(), String> {
        let event = PolicyUpdateEvent::PolicyCreated {
            policy_id,
            policy_name,
            timestamp: Utc::now(),
        };
        self.publish_update(event).await
    }

    /// Publish policy updated event
    pub async fn notify_policy_updated(
        &self,
        policy_id: String,
        policy_name: String,
    ) -> Result<(), String> {
        let event = PolicyUpdateEvent::PolicyUpdated {
            policy_id,
            policy_name,
            timestamp: Utc::now(),
        };
        self.publish_update(event).await
    }

    /// Publish policy deleted event
    pub async fn notify_policy_deleted(
        &self,
        policy_id: String,
        policy_name: String,
    ) -> Result<(), String> {
        let event = PolicyUpdateEvent::PolicyDeleted {
            policy_id,
            policy_name,
            timestamp: Utc::now(),
        };
        self.publish_update(event).await
    }

    /// Publish policy activated event
    pub async fn notify_policy_activated(
        &self,
        policy_id: String,
        policy_name: String,
    ) -> Result<(), String> {
        let event = PolicyUpdateEvent::PolicyActivated {
            policy_id,
            policy_name,
            timestamp: Utc::now(),
        };
        self.publish_update(event).await
    }

    /// Publish policy deactivated event
    pub async fn notify_policy_deactivated(
        &self,
        policy_id: String,
        policy_name: String,
    ) -> Result<(), String> {
        let event = PolicyUpdateEvent::PolicyDeactivated {
            policy_id,
            policy_name,
            timestamp: Utc::now(),
        };
        self.publish_update(event).await
    }

    /// Publish cache invalidation event
    pub async fn notify_cache_invalidation(&self, policy_ids: Vec<String>) -> Result<(), String> {
        let event = PolicyUpdateEvent::CacheInvalidation {
            policy_ids,
            timestamp: Utc::now(),
        };
        self.publish_update(event).await
    }
}

impl Default for PolicyUpdateManager {
    fn default() -> Self {
        Self::new(1000, 10000) // Default: 1000 channel capacity, 10000 history size
    }
}

/// Cache invalidation listener
pub struct CacheInvalidationListener {
    cache_name: String,
}

impl CacheInvalidationListener {
    pub fn new(cache_name: String) -> Self {
        Self { cache_name }
    }
}

impl PolicyUpdateListener for CacheInvalidationListener {
    fn handle_update(&self, event: &PolicyUpdateEvent) -> Result<(), String> {
        match event {
            PolicyUpdateEvent::PolicyCreated { policy_id, .. }
            | PolicyUpdateEvent::PolicyUpdated { policy_id, .. }
            | PolicyUpdateEvent::PolicyDeleted { policy_id, .. }
            | PolicyUpdateEvent::PolicyActivated { policy_id, .. }
            | PolicyUpdateEvent::PolicyDeactivated { policy_id, .. } => {
                tracing::info!(
                    "Cache {} invalidating policy: {}",
                    self.cache_name,
                    policy_id
                );
                // Cache invalidation is handled by the CacheInvalidationManager
                // which is integrated with the PolicyRepository
                // This listener provides notification logging for cache events
                Ok(())
            }
            PolicyUpdateEvent::CacheInvalidation { policy_ids, .. } => {
                tracing::info!(
                    "Cache {} invalidating {} policies",
                    self.cache_name,
                    policy_ids.len()
                );
                // Cache invalidation is handled by the CacheInvalidationManager
                // which is integrated with the PolicyRepository
                // This listener provides notification logging for cache events
                Ok(())
            }
        }
    }
}

/// Audit logging listener
pub struct AuditLoggingListener;

impl PolicyUpdateListener for AuditLoggingListener {
    fn handle_update(&self, event: &PolicyUpdateEvent) -> Result<(), String> {
        match event {
            PolicyUpdateEvent::PolicyCreated {
                policy_id,
                policy_name,
                timestamp,
            } => {
                tracing::info!(
                    "AUDIT: Policy created - ID: {}, Name: {}, Time: {}",
                    policy_id,
                    policy_name,
                    timestamp
                );
            }
            PolicyUpdateEvent::PolicyUpdated {
                policy_id,
                policy_name,
                timestamp,
            } => {
                tracing::info!(
                    "AUDIT: Policy updated - ID: {}, Name: {}, Time: {}",
                    policy_id,
                    policy_name,
                    timestamp
                );
            }
            PolicyUpdateEvent::PolicyDeleted {
                policy_id,
                policy_name,
                timestamp,
            } => {
                tracing::info!(
                    "AUDIT: Policy deleted - ID: {}, Name: {}, Time: {}",
                    policy_id,
                    policy_name,
                    timestamp
                );
            }
            PolicyUpdateEvent::PolicyActivated {
                policy_id,
                policy_name,
                timestamp,
            } => {
                tracing::info!(
                    "AUDIT: Policy activated - ID: {}, Name: {}, Time: {}",
                    policy_id,
                    policy_name,
                    timestamp
                );
            }
            PolicyUpdateEvent::PolicyDeactivated {
                policy_id,
                policy_name,
                timestamp,
            } => {
                tracing::info!(
                    "AUDIT: Policy deactivated - ID: {}, Name: {}, Time: {}",
                    policy_id,
                    policy_name,
                    timestamp
                );
            }
            PolicyUpdateEvent::CacheInvalidation {
                policy_ids,
                timestamp,
            } => {
                tracing::info!(
                    "AUDIT: Cache invalidation - Policies: {:?}, Time: {}",
                    policy_ids,
                    timestamp
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_policy_update_manager() {
        let manager = PolicyUpdateManager::new(10, 100);

        // Test listener registration
        let listener = Arc::new(AuditLoggingListener);
        assert!(manager
            .register_listener("audit".to_string(), listener)
            .await
            .is_ok());
        assert_eq!(manager.get_listener_count().await, 1);

        // Test policy update notification
        assert!(manager
            .notify_policy_created("policy1".to_string(), "Test Policy".to_string())
            .await
            .is_ok());

        // Test history
        let history = manager.get_update_history().await;
        assert_eq!(history.len(), 1);

        // Test listener unregistration
        assert!(manager.unregister_listener("audit").await.is_ok());
        assert_eq!(manager.get_listener_count().await, 0);
    }

    #[tokio::test]
    async fn test_broadcast_subscription() {
        let manager = PolicyUpdateManager::new(10, 100);
        let mut receiver = manager.subscribe();

        // Spawn a task to send an update
        let manager_clone = manager.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(10)).await;
            let _ = manager_clone
                .notify_policy_updated("policy1".to_string(), "Updated Policy".to_string())
                .await;
        });

        // Receive the update
        let event = receiver.recv().await.unwrap();
        match event {
            PolicyUpdateEvent::PolicyUpdated { policy_id, .. } => {
                assert_eq!(policy_id, "policy1");
            }
            _ => panic!("Expected PolicyUpdated event"),
        }
    }
}

impl Clone for PolicyUpdateManager {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            listeners: Arc::clone(&self.listeners),
            update_history: Arc::clone(&self.update_history),
            max_history_size: self.max_history_size,
        }
    }
}
