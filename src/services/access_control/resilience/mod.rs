//! Access Control Service Module
//!
//! This module provides comprehensive error handling and recovery mechanisms
//! for the hybrid access control system, including graceful degradation
//! and circuit breaker patterns.

pub mod circuit_breaker;
pub mod graceful_degradation;
pub mod health_monitor;

pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerState, CircuitBreakerStatistics,
    HealthCheckResult,
};
pub use graceful_degradation::{
    DegradationState, DegradationStatistics, DegradationStrategy, GracefulDegradationManager,
};
pub use health_monitor::{AutoRecoveryService, HealthMonitorService, MonitoringStatistics};

/// Re-export circuit breaker demonstration from examples
pub async fn run_circuit_breaker_demonstration() -> crate::errors::access_control::AccessControlResult<()> {
    tracing::info!("Circuit breaker demonstration is available in examples/circuit_breaker_demo.rs");
    tracing::info!("Run with: cargo run --example circuit_breaker_demo");
    Ok(())
}

use crate::errors::access_control::{AccessControlError, AccessControlResult};
use std::sync::Arc;
use tracing::{error, info};

/// Comprehensive error handling and recovery manager for access control
pub struct AccessControlErrorHandler {
    degradation_manager: Arc<GracefulDegradationManager>,
    rbac_circuit_breaker: Arc<CircuitBreaker>,
    abac_circuit_breaker: Arc<CircuitBreaker>,
    cache_circuit_breaker: Arc<CircuitBreaker>,
    database_circuit_breaker: Arc<CircuitBreaker>,
}

impl AccessControlErrorHandler {
    pub fn new(
        degradation_strategy: DegradationStrategy,
        circuit_breaker_config: CircuitBreakerConfig,
    ) -> Self {
        let degradation_manager = Arc::new(GracefulDegradationManager::new(
            degradation_strategy,
            5,                            // failure threshold
            chrono::Duration::minutes(5), // recovery timeout
        ));

        let rbac_circuit_breaker = Arc::new(CircuitBreaker::new(
            "RBAC".to_string(),
            circuit_breaker_config.clone(),
        ));

        let abac_circuit_breaker = Arc::new(CircuitBreaker::new(
            "ABAC".to_string(),
            circuit_breaker_config.clone(),
        ));

        let cache_circuit_breaker = Arc::new(CircuitBreaker::new(
            "Cache".to_string(),
            circuit_breaker_config.clone(),
        ));

        let database_circuit_breaker = Arc::new(CircuitBreaker::new(
            "Database".to_string(),
            circuit_breaker_config,
        ));

        Self {
            degradation_manager,
            rbac_circuit_breaker,
            abac_circuit_breaker,
            cache_circuit_breaker,
            database_circuit_breaker,
        }
    }

    /// Execute RBAC operation with circuit breaker protection
    pub async fn execute_rbac<T, F, Fut>(&self, operation: F) -> AccessControlResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AccessControlResult<T>>,
    {
        self.rbac_circuit_breaker.execute(operation).await
    }

    /// Execute ABAC operation with circuit breaker protection and graceful degradation
    pub async fn execute_abac_with_fallback<T, F, R, FutA, FutR>(
        &self,
        abac_operation: F,
        rbac_fallback: R,
    ) -> AccessControlResult<T>
    where
        F: FnOnce() -> FutA,
        R: FnOnce() -> FutR,
        FutA: std::future::Future<Output = AccessControlResult<T>>,
        FutR: std::future::Future<Output = AccessControlResult<T>>,
    {
        let abac_future = self.abac_circuit_breaker.execute(abac_operation);
        let rbac_future = self.rbac_circuit_breaker.execute(rbac_fallback);

        self.degradation_manager
            .handle_abac_failure(abac_future, rbac_future, "access_evaluation")
            .await
    }

    /// Execute cache operation with circuit breaker protection
    pub async fn execute_cache<T, F, Fut>(&self, operation: F) -> AccessControlResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AccessControlResult<T>>,
    {
        self.cache_circuit_breaker.execute(operation).await
    }

    /// Execute database operation with circuit breaker protection
    pub async fn execute_database<T, F, Fut>(&self, operation: F) -> AccessControlResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AccessControlResult<T>>,
    {
        self.database_circuit_breaker.execute(operation).await
    }

    /// Handle critical system error with appropriate recovery strategy
    pub async fn handle_critical_error<T>(
        &self,
        error: AccessControlError,
        context: &str,
    ) -> AccessControlResult<T> {
        error!("Critical access control error in {}: {}", context, error);

        // Record the error for degradation tracking
        self.degradation_manager.record_failure(&error).await?;

        // Apply system-wide recovery strategy
        self.degradation_manager
            .handle_system_failure(error, context)
            .await
    }

    /// Get comprehensive system health status
    pub async fn get_health_status(&self) -> SystemHealthStatus {
        let degradation_stats = self.degradation_manager.get_statistics().await;
        let rbac_stats = self.rbac_circuit_breaker.get_statistics().await;
        let abac_stats = self.abac_circuit_breaker.get_statistics().await;
        let cache_stats = self.cache_circuit_breaker.get_statistics().await;
        let database_stats = self.database_circuit_breaker.get_statistics().await;

        let overall_health = !degradation_stats.is_degraded
            && rbac_stats.state == CircuitBreakerState::Closed
            && abac_stats.state == CircuitBreakerState::Closed
            && cache_stats.state == CircuitBreakerState::Closed
            && database_stats.state == CircuitBreakerState::Closed;

        SystemHealthStatus {
            overall_health,
            degradation: degradation_stats,
            circuit_breakers: vec![rbac_stats, abac_stats, cache_stats, database_stats],
            timestamp: chrono::Utc::now(),
        }
    }

    /// Perform comprehensive health check
    pub async fn perform_health_check(&self) -> HealthCheckSummary {
        let mut results = Vec::new();

        // Check RBAC circuit breaker
        let rbac_health = self
            .rbac_circuit_breaker
            .health_check(|| async { Ok(()) })
            .await;
        results.push(rbac_health);

        // Check ABAC circuit breaker
        let abac_health = self
            .abac_circuit_breaker
            .health_check(|| async { Ok(()) })
            .await;
        results.push(abac_health);

        // Check cache circuit breaker
        let cache_health = self
            .cache_circuit_breaker
            .health_check(|| async { Ok(()) })
            .await;
        results.push(cache_health);

        // Check database circuit breaker
        let database_health = self
            .database_circuit_breaker
            .health_check(|| async { Ok(()) })
            .await;
        results.push(database_health);

        let all_healthy = results.iter().all(|r| r.is_healthy);
        let degradation_state = self.degradation_manager.get_state().await;

        HealthCheckSummary {
            overall_healthy: all_healthy && !degradation_state.is_degraded,
            component_results: results,
            degradation_state,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Force recovery of all components (admin operation)
    pub async fn force_recovery(&self) -> AccessControlResult<()> {
        info!("Forcing recovery of all access control components");

        // Force degradation manager recovery
        self.degradation_manager.force_recovery().await?;

        // Reset all circuit breakers
        self.rbac_circuit_breaker.reset().await?;
        self.abac_circuit_breaker.reset().await?;
        self.cache_circuit_breaker.reset().await?;
        self.database_circuit_breaker.reset().await?;

        info!("All access control components have been reset");
        Ok(())
    }

    /// Get error handling statistics for monitoring
    pub async fn get_error_statistics(&self) -> ErrorHandlingStatistics {
        let degradation_stats = self.degradation_manager.get_statistics().await;
        let rbac_stats = self.rbac_circuit_breaker.get_statistics().await;
        let abac_stats = self.abac_circuit_breaker.get_statistics().await;
        let cache_stats = self.cache_circuit_breaker.get_statistics().await;
        let database_stats = self.database_circuit_breaker.get_statistics().await;

        ErrorHandlingStatistics {
            degradation: degradation_stats,
            rbac_circuit_breaker: rbac_stats,
            abac_circuit_breaker: abac_stats,
            cache_circuit_breaker: cache_stats,
            database_circuit_breaker: database_stats,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// System health status for monitoring
#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemHealthStatus {
    pub overall_health: bool,
    pub degradation: DegradationStatistics,
    pub circuit_breakers: Vec<CircuitBreakerStatistics>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Health check summary
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthCheckSummary {
    pub overall_healthy: bool,
    pub component_results: Vec<HealthCheckResult>,
    pub degradation_state: DegradationState,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Error handling statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct ErrorHandlingStatistics {
    pub degradation: DegradationStatistics,
    pub rbac_circuit_breaker: CircuitBreakerStatistics,
    pub abac_circuit_breaker: CircuitBreakerStatistics,
    pub cache_circuit_breaker: CircuitBreakerStatistics,
    pub database_circuit_breaker: CircuitBreakerStatistics,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Default for AccessControlErrorHandler {
    fn default() -> Self {
        Self::new(
            DegradationStrategy::FallbackToRbac,
            CircuitBreakerConfig::default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_error_handler_creation() {
        let handler = AccessControlErrorHandler::default();
        let health = handler.get_health_status().await;

        assert!(health.overall_health);
        assert!(!health.degradation.is_degraded);
        assert_eq!(health.circuit_breakers.len(), 4);
    }

    #[tokio::test]
    async fn test_rbac_circuit_breaker_execution() {
        let handler = AccessControlErrorHandler::default();

        let result = handler
            .execute_rbac(|| async { Ok::<String, AccessControlError>("success".to_string()) })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_abac_fallback_to_rbac() {
        let handler = AccessControlErrorHandler::new(
            DegradationStrategy::FallbackToRbac,
            CircuitBreakerConfig::default(),
        );

        let result = handler
            .execute_abac_with_fallback(
                || async {
                    Err::<String, _>(AccessControlError::AbacError {
                        message: "ABAC failed".to_string(),
                        source: None,
                        context: crate::errors::context::ErrorContext::new("test"),
                    })
                },
                || async { Ok::<String, _>("RBAC success".to_string()) },
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "RBAC success");
    }
}
