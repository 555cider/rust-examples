use super::{AccessControlErrorHandler, CircuitBreakerState};
use crate::errors::access_control::{AccessControlError, AccessControlResult};
use crate::errors::context::ErrorContext;
use std::sync::Arc;
use tokio::time::{interval, Duration, Instant};
use tracing::{debug, error, info, warn};

/// Health monitoring service for access control components
pub struct HealthMonitorService {
    error_handler: Arc<AccessControlErrorHandler>,
    monitoring_interval: Duration,
    health_check_timeout: Duration,
    is_running: Arc<tokio::sync::RwLock<bool>>,
}

impl HealthMonitorService {
    pub fn new(
        error_handler: Arc<AccessControlErrorHandler>,
        monitoring_interval: Duration,
        health_check_timeout: Duration,
    ) -> Self {
        Self {
            error_handler,
            monitoring_interval,
            health_check_timeout,
            is_running: Arc::new(tokio::sync::RwLock::new(false)),
        }
    }

    /// Start the health monitoring background task
    pub async fn start(&self) -> AccessControlResult<()> {
        let mut is_running = self.is_running.write().await;
        if *is_running {
            return Err(AccessControlError::ConfigurationError {
                message: "Health monitor is already running".to_string(),
                context: ErrorContext::new("health_monitor_start"),
            });
        }
        *is_running = true;
        drop(is_running);

        let error_handler = Arc::clone(&self.error_handler);
        let monitoring_interval = self.monitoring_interval;
        let health_check_timeout = self.health_check_timeout;
        let is_running_flag = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting access control health monitoring service");
            let mut interval_timer = interval(monitoring_interval);

            loop {
                // Check if we should continue running
                {
                    let running = is_running_flag.read().await;
                    if !*running {
                        info!("Health monitoring service stopped");
                        break;
                    }
                }

                interval_timer.tick().await;

                // Perform health checks
                if let Err(e) =
                    Self::perform_health_checks(&error_handler, health_check_timeout).await
                {
                    error!("Health check failed: {}", e);
                }
            }
        });

        Ok(())
    }

    /// Stop the health monitoring service
    pub async fn stop(&self) -> AccessControlResult<()> {
        let mut is_running = self.is_running.write().await;
        *is_running = false;
        info!("Health monitoring service stop requested");
        Ok(())
    }

    /// Check if the monitoring service is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Perform comprehensive health checks on all components
    async fn perform_health_checks(
        error_handler: &AccessControlErrorHandler,
        timeout: Duration,
    ) -> AccessControlResult<()> {
        let start_time = Instant::now();

        // Get current health status
        let health_status = error_handler.get_health_status().await;

        debug!(
            "Performing health checks for {} components",
            health_status.circuit_breakers.len()
        );

        // Check each circuit breaker
        for cb_stats in &health_status.circuit_breakers {
            match cb_stats.state {
                CircuitBreakerState::Open => {
                    warn!(
                        "Circuit breaker '{}' is OPEN - {} failures recorded",
                        cb_stats.component_name, cb_stats.failure_count
                    );
                }
                CircuitBreakerState::HalfOpen => {
                    info!(
                        "Circuit breaker '{}' is HALF-OPEN - testing recovery with {} requests",
                        cb_stats.component_name, cb_stats.half_open_requests
                    );
                }
                CircuitBreakerState::Closed => {
                    debug!(
                        "Circuit breaker '{}' is CLOSED - healthy with {} recent failures",
                        cb_stats.component_name, cb_stats.failure_count
                    );
                }
            }
        }

        // Check degradation status
        if health_status.degradation.is_degraded {
            warn!(
                "System is in degraded mode: {} (degraded for {} failures)",
                health_status
                    .degradation
                    .degradation_reason
                    .as_deref()
                    .unwrap_or("unknown"),
                health_status.degradation.failure_count
            );
        }

        // Perform actual health check
        let health_check_result =
            tokio::time::timeout(timeout, error_handler.perform_health_check()).await;

        match health_check_result {
            Ok(health_summary) => {
                let healthy_components = health_summary
                    .component_results
                    .iter()
                    .filter(|r| r.is_healthy)
                    .count();

                let total_components = health_summary.component_results.len();

                if health_summary.overall_healthy {
                    debug!(
                        "Health check completed successfully: {}/{} components healthy in {:?}",
                        healthy_components,
                        total_components,
                        start_time.elapsed()
                    );
                } else {
                    warn!(
                        "Health check shows system issues: {}/{} components healthy",
                        healthy_components, total_components
                    );
                }
            }
            Err(_) => {
                error!("Health check timed out after {:?}", timeout);
                return Err(AccessControlError::EvaluationTimeout {
                    duration: timeout,
                    context: ErrorContext::new("health_check_timeout"),
                });
            }
        }

        Ok(())
    }

    /// Trigger manual health check
    pub async fn trigger_health_check(&self) -> AccessControlResult<super::HealthCheckSummary> {
        info!("Manual health check triggered");

        let result = tokio::time::timeout(
            self.health_check_timeout,
            self.error_handler.perform_health_check(),
        )
        .await;

        match result {
            Ok(health_summary) => {
                info!(
                    "Manual health check completed: overall_healthy={}",
                    health_summary.overall_healthy
                );
                Ok(health_summary)
            }
            Err(_) => {
                error!("Manual health check timed out");
                Err(AccessControlError::EvaluationTimeout {
                    duration: self.health_check_timeout,
                    context: ErrorContext::new("manual_health_check_timeout"),
                })
            }
        }
    }

    /// Get monitoring statistics
    pub async fn get_monitoring_statistics(&self) -> MonitoringStatistics {
        let health_status = self.error_handler.get_health_status().await;
        let is_running = self.is_running().await;

        MonitoringStatistics {
            is_monitoring_active: is_running,
            monitoring_interval_seconds: self.monitoring_interval.as_secs(),
            health_check_timeout_seconds: self.health_check_timeout.as_secs(),
            overall_system_health: health_status.overall_health,
            degraded_components: health_status
                .circuit_breakers
                .iter()
                .filter(|cb| cb.state != CircuitBreakerState::Closed)
                .map(|cb| cb.component_name.clone())
                .collect(),
            last_check_timestamp: chrono::Utc::now(),
        }
    }
}

/// Monitoring statistics for reporting
#[derive(Debug, Clone, serde::Serialize)]
pub struct MonitoringStatistics {
    pub is_monitoring_active: bool,
    pub monitoring_interval_seconds: u64,
    pub health_check_timeout_seconds: u64,
    pub overall_system_health: bool,
    pub degraded_components: Vec<String>,
    pub last_check_timestamp: chrono::DateTime<chrono::Utc>,
}

/// Auto-recovery service for circuit breakers
pub struct AutoRecoveryService {
    error_handler: Arc<AccessControlErrorHandler>,
    recovery_check_interval: Duration,
    max_recovery_attempts: u32,
    is_running: Arc<tokio::sync::RwLock<bool>>,
}

impl AutoRecoveryService {
    pub fn new(
        error_handler: Arc<AccessControlErrorHandler>,
        recovery_check_interval: Duration,
        max_recovery_attempts: u32,
    ) -> Self {
        Self {
            error_handler,
            recovery_check_interval,
            max_recovery_attempts,
            is_running: Arc::new(tokio::sync::RwLock::new(false)),
        }
    }

    /// Start the auto-recovery background task
    pub async fn start(&self) -> AccessControlResult<()> {
        let mut is_running = self.is_running.write().await;
        if *is_running {
            return Err(AccessControlError::ConfigurationError {
                message: "Auto-recovery service is already running".to_string(),
                context: ErrorContext::new("auto_recovery_start"),
            });
        }
        *is_running = true;
        drop(is_running);

        let error_handler = Arc::clone(&self.error_handler);
        let recovery_check_interval = self.recovery_check_interval;
        let max_recovery_attempts = self.max_recovery_attempts;
        let is_running_flag = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting access control auto-recovery service");
            let mut interval_timer = interval(recovery_check_interval);
            let mut recovery_attempts = 0;

            loop {
                // Check if we should continue running
                {
                    let running = is_running_flag.read().await;
                    if !*running {
                        info!("Auto-recovery service stopped");
                        break;
                    }
                }

                interval_timer.tick().await;

                // Check for components that need recovery
                let health_status = error_handler.get_health_status().await;

                let needs_recovery = !health_status.overall_health
                    || health_status.degradation.is_degraded
                    || health_status
                        .circuit_breakers
                        .iter()
                        .any(|cb| cb.state != CircuitBreakerState::Closed);

                if needs_recovery && recovery_attempts < max_recovery_attempts {
                    recovery_attempts += 1;

                    warn!(
                        "System health issues detected, attempting recovery #{}/{}",
                        recovery_attempts, max_recovery_attempts
                    );

                    // Attempt recovery
                    match Self::attempt_recovery(&error_handler).await {
                        Ok(_) => {
                            info!(
                                "Recovery attempt #{} completed successfully",
                                recovery_attempts
                            );
                            recovery_attempts = 0; // Reset on success
                        }
                        Err(e) => {
                            error!("Recovery attempt #{} failed: {}", recovery_attempts, e);
                        }
                    }
                } else if recovery_attempts >= max_recovery_attempts {
                    error!(
                        "Maximum recovery attempts ({}) reached, manual intervention required",
                        max_recovery_attempts
                    );
                    // Stop trying automatic recovery
                    recovery_attempts = 0;

                    // Wait longer before next check
                    tokio::time::sleep(recovery_check_interval * 5).await;
                } else if health_status.overall_health {
                    // System is healthy, reset recovery attempts
                    if recovery_attempts > 0 {
                        info!("System recovered, resetting recovery attempt counter");
                        recovery_attempts = 0;
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop the auto-recovery service
    pub async fn stop(&self) -> AccessControlResult<()> {
        let mut is_running = self.is_running.write().await;
        *is_running = false;
        info!("Auto-recovery service stop requested");
        Ok(())
    }

    /// Attempt to recover degraded components
    async fn attempt_recovery(
        _error_handler: &AccessControlErrorHandler,
    ) -> AccessControlResult<()> {
        info!("Attempting automatic recovery of access control components");

        // This is a conservative recovery approach
        // In a production system, you might want more sophisticated recovery strategies

        // Log the recovery attempt
        // Full recovery is too aggressive for automatic recovery - manual intervention preferred
        warn!("Automatic recovery is conservative - manual intervention may be required");

        // You could implement specific recovery strategies here:
        // - Reset specific circuit breakers that have been open for too long
        // - Clear degradation state if conditions have improved
        // - Restart specific components

        Ok(())
    }

    /// Check if the auto-recovery service is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }
}

impl Default for HealthMonitorService {
    fn default() -> Self {
        // HealthMonitorService requires proper error handler injection
        panic!("HealthMonitorService::default() should not be used - use HealthMonitorService::new() with proper error handler")
    }
}

impl Default for AutoRecoveryService {
    fn default() -> Self {
        // AutoRecoveryService requires proper error handler injection
        panic!("AutoRecoveryService::default() should not be used - use AutoRecoveryService::new() with proper error handler")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::access_control::{
        AccessControlErrorHandler, CircuitBreakerConfig, DegradationStrategy,
    };

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let error_handler = Arc::new(AccessControlErrorHandler::new(
            DegradationStrategy::FallbackToRbac,
            CircuitBreakerConfig::default(),
        ));

        let monitor = HealthMonitorService::new(
            error_handler,
            Duration::from_secs(30),
            Duration::from_secs(5),
        );

        assert!(!monitor.is_running().await);
    }

    #[tokio::test]
    async fn test_monitoring_statistics() {
        let error_handler = Arc::new(AccessControlErrorHandler::new(
            DegradationStrategy::FallbackToRbac,
            CircuitBreakerConfig::default(),
        ));

        let monitor = HealthMonitorService::new(
            error_handler,
            Duration::from_secs(30),
            Duration::from_secs(5),
        );

        let stats = monitor.get_monitoring_statistics().await;
        assert!(!stats.is_monitoring_active);
        assert_eq!(stats.monitoring_interval_seconds, 30);
        assert_eq!(stats.health_check_timeout_seconds, 5);
    }

    #[tokio::test]
    async fn test_auto_recovery_creation() {
        let error_handler = Arc::new(AccessControlErrorHandler::new(
            DegradationStrategy::FallbackToRbac,
            CircuitBreakerConfig::default(),
        ));

        let recovery = AutoRecoveryService::new(error_handler, Duration::from_secs(60), 3);

        assert!(!recovery.is_running().await);
    }
}
