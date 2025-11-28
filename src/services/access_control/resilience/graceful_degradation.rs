use crate::errors::access_control::{AccessControlError, AccessControlResult};
use crate::errors::context::ErrorContext;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Graceful degradation strategy for access control failures
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradationStrategy {
    /// Fail completely - no fallback
    FailFast,
    /// Fallback to RBAC only when ABAC fails
    FallbackToRbac,
    /// Allow access when both systems fail (emergency mode)
    AllowOnFailure,
    /// Deny access when both systems fail (secure mode)
    DenyOnFailure,
}

impl std::fmt::Display for DegradationStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DegradationStrategy::FailFast => write!(f, "fail_fast"),
            DegradationStrategy::FallbackToRbac => write!(f, "fallback_to_rbac"),
            DegradationStrategy::AllowOnFailure => write!(f, "allow_on_failure"),
            DegradationStrategy::DenyOnFailure => write!(f, "deny_on_failure"),
        }
    }
}

/// Degradation state tracking
#[derive(Debug, Clone, serde::Serialize)]
pub struct DegradationState {
    pub is_degraded: bool,
    pub degradation_reason: Option<String>,
    pub degraded_since: Option<chrono::DateTime<chrono::Utc>>,
    pub failure_count: u32,
    pub last_failure: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for DegradationState {
    fn default() -> Self {
        Self {
            is_degraded: false,
            degradation_reason: None,
            degraded_since: None,
            failure_count: 0,
            last_failure: None,
        }
    }
}

/// Graceful degradation manager for access control system
pub struct GracefulDegradationManager {
    strategy: DegradationStrategy,
    state: Arc<RwLock<DegradationState>>,
    failure_threshold: u32,
    recovery_timeout: chrono::Duration,
}

impl GracefulDegradationManager {
    pub fn new(
        strategy: DegradationStrategy,
        failure_threshold: u32,
        recovery_timeout: chrono::Duration,
    ) -> Self {
        Self {
            strategy,
            state: Arc::new(RwLock::new(DegradationState::default())),
            failure_threshold,
            recovery_timeout,
        }
    }

    /// Record a failure and potentially trigger degradation
    pub async fn record_failure(&self, error: &AccessControlError) -> AccessControlResult<()> {
        let mut state = self.state.write().await;

        state.failure_count += 1;
        state.last_failure = Some(chrono::Utc::now());

        // Check if we should enter degraded mode
        if !state.is_degraded && state.failure_count >= self.failure_threshold {
            state.is_degraded = true;
            state.degraded_since = Some(chrono::Utc::now());
            state.degradation_reason = Some(error.to_string());

            warn!(
                "Access control system entering degraded mode after {} failures. Reason: {}",
                state.failure_count, error
            );
        }

        Ok(())
    }

    /// Record a successful operation and potentially recover from degradation
    pub async fn record_success(&self) -> AccessControlResult<()> {
        let mut state = self.state.write().await;

        if state.is_degraded {
            // Check if enough time has passed for recovery
            if let Some(degraded_since) = state.degraded_since {
                let now = chrono::Utc::now();
                if now.signed_duration_since(degraded_since) >= self.recovery_timeout {
                    state.is_degraded = false;
                    state.degraded_since = None;
                    state.degradation_reason = None;
                    state.failure_count = 0;

                    info!("Access control system recovered from degraded mode");
                }
            }
        } else {
            // Reset failure count on successful operation
            state.failure_count = 0;
        }

        Ok(())
    }

    /// Check if the system is currently in degraded mode
    pub async fn is_degraded(&self) -> bool {
        self.state.read().await.is_degraded
    }

    /// Get current degradation state
    pub async fn get_state(&self) -> DegradationState {
        self.state.read().await.clone()
    }

    /// Handle ABAC failure with graceful degradation
    pub async fn handle_abac_failure<T, F, R>(
        &self,
        abac_operation: F,
        rbac_fallback: R,
        context: &str,
    ) -> AccessControlResult<T>
    where
        F: std::future::Future<Output = AccessControlResult<T>>,
        R: std::future::Future<Output = AccessControlResult<T>>,
    {
        match abac_operation.await {
            Ok(result) => {
                self.record_success().await?;
                Ok(result)
            }
            Err(abac_error) => {
                self.record_failure(&abac_error).await?;

                match self.strategy {
                    DegradationStrategy::FailFast => {
                        error!("ABAC failure in {}: {}", context, abac_error);
                        Err(abac_error)
                    }
                    DegradationStrategy::FallbackToRbac => {
                        warn!(
                            "ABAC failure in {}, falling back to RBAC: {}",
                            context, abac_error
                        );

                        match rbac_fallback.await {
                            Ok(result) => Ok(result),
                            Err(rbac_error) => {
                                error!("RBAC fallback also failed in {}: {}", context, rbac_error);
                                Err(AccessControlError::FallbackFailed {
                                    message: format!(
                                        "ABAC failed: {}, RBAC fallback failed: {}",
                                        abac_error, rbac_error
                                    ),
                                    source: None,
                                    context: ErrorContext::new("abac_rbac_fallback"),
                                })
                            }
                        }
                    }
                    DegradationStrategy::AllowOnFailure => {
                        warn!(
                            "ABAC failure in {}, allowing access due to emergency mode: {}",
                            context, abac_error
                        );
                        // AllowOnFailure strategy returns error - specific implementation depends on context
                        Err(AccessControlError::GracefulDegradationUnavailable {
                            message: "AllowOnFailure strategy requires specific implementation"
                                .to_string(),
                            context: ErrorContext::new("allow_on_failure"),
                        })
                    }
                    DegradationStrategy::DenyOnFailure => {
                        warn!(
                            "ABAC failure in {}, denying access due to secure mode: {}",
                            context, abac_error
                        );
                        Err(AccessControlError::AccessDenied {
                            reason: format!("Access denied due to system failure: {}", abac_error),
                            context: ErrorContext::new("deny_on_failure"),
                        })
                    }
                }
            }
        }
    }

    /// Handle complete system failure
    pub async fn handle_system_failure<T>(
        &self,
        error: AccessControlError,
        context: &str,
    ) -> AccessControlResult<T> {
        self.record_failure(&error).await?;

        match self.strategy {
            DegradationStrategy::AllowOnFailure => {
                warn!(
                    "Complete system failure in {}, allowing access due to emergency mode: {}",
                    context, error
                );
                Err(AccessControlError::GracefulDegradationUnavailable {
                    message: "AllowOnFailure strategy requires specific implementation".to_string(),
                    context: ErrorContext::new("system_failure_allow"),
                })
            }
            _ => {
                error!("Complete system failure in {}: {}", context, error);
                Err(AccessControlError::AccessDenied {
                    reason: format!("Access denied due to complete system failure: {}", error),
                    context: ErrorContext::new("system_failure_deny"),
                })
            }
        }
    }

    /// Force recovery from degraded mode (admin operation)
    pub async fn force_recovery(&self) -> AccessControlResult<()> {
        let mut state = self.state.write().await;

        if state.is_degraded {
            state.is_degraded = false;
            state.degraded_since = None;
            state.degradation_reason = None;
            state.failure_count = 0;

            info!("Access control system manually recovered from degraded mode");
        }

        Ok(())
    }

    /// Get degradation statistics for monitoring
    pub async fn get_statistics(&self) -> DegradationStatistics {
        let state = self.state.read().await;

        DegradationStatistics {
            is_degraded: state.is_degraded,
            failure_count: state.failure_count,
            degraded_since: state.degraded_since,
            last_failure: state.last_failure,
            degradation_reason: state.degradation_reason.clone(),
            strategy: self.strategy,
            failure_threshold: self.failure_threshold,
            recovery_timeout_seconds: self.recovery_timeout.num_seconds(),
        }
    }
}

/// Statistics for monitoring and alerting
#[derive(Debug, Clone, serde::Serialize)]
pub struct DegradationStatistics {
    pub is_degraded: bool,
    pub failure_count: u32,
    pub degraded_since: Option<chrono::DateTime<chrono::Utc>>,
    pub last_failure: Option<chrono::DateTime<chrono::Utc>>,
    pub degradation_reason: Option<String>,
    pub strategy: DegradationStrategy,
    pub failure_threshold: u32,
    pub recovery_timeout_seconds: i64,
}

impl serde::Serialize for DegradationStrategy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            DegradationStrategy::FailFast => "fail_fast",
            DegradationStrategy::FallbackToRbac => "fallback_to_rbac",
            DegradationStrategy::AllowOnFailure => "allow_on_failure",
            DegradationStrategy::DenyOnFailure => "deny_on_failure",
        };
        serializer.serialize_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_degradation_on_failures() {
        let manager = GracefulDegradationManager::new(
            DegradationStrategy::FallbackToRbac,
            3, // failure threshold
            chrono::Duration::seconds(60),
        );

        // Should not be degraded initially
        assert!(!manager.is_degraded().await);

        // Record failures
        let error = AccessControlError::AbacError {
            message: "test error".to_string(),
            source: None,
            context: crate::errors::context::ErrorContext::new("test"),
        };
        for _ in 0..2 {
            manager.record_failure(&error).await.unwrap();
        }

        // Should not be degraded yet
        assert!(!manager.is_degraded().await);

        // Third failure should trigger degradation
        manager.record_failure(&error).await.unwrap();
        assert!(manager.is_degraded().await);
    }

    #[tokio::test]
    async fn test_recovery_after_success() {
        let manager = GracefulDegradationManager::new(
            DegradationStrategy::FallbackToRbac,
            2,
            chrono::Duration::milliseconds(100), // Short timeout for testing
        );

        // Trigger degradation
        let error = AccessControlError::AbacError {
            message: "test error".to_string(),
            source: None,
            context: crate::errors::context::ErrorContext::new("test"),
        };
        for _ in 0..2 {
            manager.record_failure(&error).await.unwrap();
        }
        assert!(manager.is_degraded().await);

        // Wait for recovery timeout
        sleep(Duration::from_millis(150)).await;

        // Record success should trigger recovery
        manager.record_success().await.unwrap();
        assert!(!manager.is_degraded().await);
    }

    #[tokio::test]
    async fn test_force_recovery() {
        let manager = GracefulDegradationManager::new(
            DegradationStrategy::FallbackToRbac,
            1,
            chrono::Duration::hours(1), // Long timeout
        );

        // Trigger degradation
        let error = AccessControlError::AbacError {
            message: "test error".to_string(),
            source: None,
            context: crate::errors::context::ErrorContext::new("test"),
        };
        manager.record_failure(&error).await.unwrap();
        assert!(manager.is_degraded().await);

        // Force recovery
        manager.force_recovery().await.unwrap();
        assert!(!manager.is_degraded().await);
    }
}
