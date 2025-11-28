use super::*;

impl RbacEngine {
    /// Invalidate cache for a specific user
    pub async fn invalidate_user_cache(&self, user_id: i64) {
        if let Some(cache) = &self.cache {
            cache.invalidate_user_cache(user_id).await;
        }

        info!(user_id = user_id, "Invalidated RBAC cache");
    }

    /// Clear all cache
    pub async fn clear_cache(&self) {
        if let Some(cache) = &self.cache {
            cache.clear_all().await;
        }

        info!("Cleared all RBAC cache");
    }
}
