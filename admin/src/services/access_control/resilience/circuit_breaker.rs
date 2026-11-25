use crate::errors::access_control::{AccessControlError, AccessControlResult};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerState {
    /// Circuit is closed - normal operation
    Closed,
    /// Circuit is open - all requests fail fast
    Open,
    /// Circuit is half-open - limited requests allowed for testing
    HalfOpen,
}

impl std::fmt::Display for CircuitBreakerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerState::Closed => write!(f, "closed"),
            CircuitBreakerState::Open => write!(f, "open"),
            CircuitBreakerState::HalfOpen => write!(f, "half_open"),
        }
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone, serde::Serialize)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit
    pub failure_threshold: u32,
    /// Time to wait before transitioning from Open to HalfOpen
    pub timeout: Duration,
    /// Number of successful requests needed to close the circuit from HalfOpen
    pub success_threshold: u32,
    /// Maximum number of requests allowed in HalfOpen state
    pub half_open_max_requests: u32,
    /// Time window for counting failures
    pub failure_window: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(60),
            success_threshold: 3,
            half_open_max_requests: 5,
            failure_window: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Internal state of the circuit breaker
#[derive(Debug, Clone)]
struct CircuitBreakerInternalState {
    state: CircuitBreakerState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
    last_state_change: Instant,
    half_open_requests: u32,
    failure_times: Vec<Instant>,
}

impl Default for CircuitBreakerInternalState {
    fn default() -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            last_state_change: Instant::now(),
            half_open_requests: 0,
            failure_times: Vec::new(),
        }
    }
}

/// Circuit breaker for access control components
pub struct CircuitBreaker {
    component_name: String,
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitBreakerInternalState>>,
}

impl CircuitBreaker {
    pub fn new(component_name: String, config: CircuitBreakerConfig) -> Self {
        Self {
            component_name,
            config,
            state: Arc::new(RwLock::new(CircuitBreakerInternalState::default())),
        }
    }

    /// Execute an operation with circuit breaker protection
    pub async fn execute<T, F, Fut>(&self, operation: F) -> AccessControlResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AccessControlResult<T>>,
    {
        // Check if we can execute the operation
        if !self.can_execute().await? {
            return Err(self.get_circuit_breaker_error().await);
        }

        // Execute the operation
        let start_time = Instant::now();
        let result = operation().await;
        let execution_time = start_time.elapsed();

        // Record the result
        match &result {
            Ok(_) => {
                self.record_success().await?;
                debug!(
                    "Circuit breaker '{}': Operation succeeded in {:?}",
                    self.component_name, execution_time
                );
            }
            Err(error) => {
                self.record_failure(error).await?;
                debug!(
                    "Circuit breaker '{}': Operation failed in {:?}: {}",
                    self.component_name, execution_time, error
                );
            }
        }

        result
    }

    /// Check if the circuit breaker allows execution
    async fn can_execute(&self) -> AccessControlResult<bool> {
        let mut state = self.state.write().await;
        let now = Instant::now();

        // Clean up old failure times
        state
            .failure_times
            .retain(|&time| now.duration_since(time) <= self.config.failure_window);
        state.failure_count = state.failure_times.len() as u32;

        match state.state {
            CircuitBreakerState::Closed => Ok(true),
            CircuitBreakerState::Open => {
                // Check if timeout has passed
                if now.duration_since(state.last_state_change) >= self.config.timeout {
                    // Transition to HalfOpen
                    state.state = CircuitBreakerState::HalfOpen;
                    state.last_state_change = now;
                    state.half_open_requests = 0;
                    state.success_count = 0;

                    info!(
                        "Circuit breaker '{}': Transitioning from Open to HalfOpen",
                        self.component_name
                    );

                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Allow limited requests in HalfOpen state
                if state.half_open_requests < self.config.half_open_max_requests {
                    state.half_open_requests += 1;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    }

    /// Record a successful operation
    async fn record_success(&self) -> AccessControlResult<()> {
        let mut state = self.state.write().await;
        let now = Instant::now();

        match state.state {
            CircuitBreakerState::Closed => {
                // Reset failure count on success
                state.failure_count = 0;
                state.failure_times.clear();
            }
            CircuitBreakerState::HalfOpen => {
                state.success_count += 1;

                // Check if we have enough successes to close the circuit
                if state.success_count >= self.config.success_threshold {
                    state.state = CircuitBreakerState::Closed;
                    state.last_state_change = now;
                    state.failure_count = 0;
                    state.success_count = 0;
                    state.half_open_requests = 0;
                    state.failure_times.clear();

                    info!(
                        "Circuit breaker '{}': Transitioning from HalfOpen to Closed after {} successes",
                        self.component_name, self.config.success_threshold
                    );
                }
            }
            CircuitBreakerState::Open => {
                // This shouldn't happen, but handle it gracefully
                warn!(
                    "Circuit breaker '{}': Received success while in Open state",
                    self.component_name
                );
            }
        }

        Ok(())
    }

    /// Record a failed operation
    async fn record_failure(&self, error: &AccessControlError) -> AccessControlResult<()> {
        let mut state = self.state.write().await;
        let now = Instant::now();

        state.failure_times.push(now);
        state.failure_count += 1;
        state.last_failure_time = Some(now);

        match state.state {
            CircuitBreakerState::Closed => {
                // Check if we should open the circuit
                if state.failure_count >= self.config.failure_threshold {
                    state.state = CircuitBreakerState::Open;
                    state.last_state_change = now;

                    error!(
                        "Circuit breaker '{}': Opening circuit after {} failures. Last error: {}",
                        self.component_name, state.failure_count, error
                    );
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Any failure in HalfOpen state opens the circuit
                state.state = CircuitBreakerState::Open;
                state.last_state_change = now;
                state.success_count = 0;
                state.half_open_requests = 0;

                warn!(
                    "Circuit breaker '{}': Reopening circuit from HalfOpen due to failure: {}",
                    self.component_name, error
                );
            }
            CircuitBreakerState::Open => {
                // Already open, just record the failure
                debug!(
                    "Circuit breaker '{}': Recording failure while circuit is open: {}",
                    self.component_name, error
                );
            }
        }

        Ok(())
    }

    /// Get the appropriate error for the current circuit breaker state
    async fn get_circuit_breaker_error(&self) -> AccessControlError {
        use crate::errors::context::ErrorContext;
        let state = self.state.read().await;

        let context = ErrorContext::new("circuit_breaker")
            .with_data("component", self.component_name.clone());

        match state.state {
            CircuitBreakerState::Open => AccessControlError::CircuitBreakerOpen {
                component: self.component_name.clone(),
                context,
            },
            CircuitBreakerState::HalfOpen => AccessControlError::CircuitBreakerHalfOpen {
                component: self.component_name.clone(),
                context,
            },
            CircuitBreakerState::Closed => {
                // This shouldn't happen
                AccessControlError::ConfigurationError {
                    message: format!(
                        "Circuit breaker '{}' is closed but execution was denied",
                        self.component_name
                    ),
                    context,
                }
            }
        }
    }

    /// Get current circuit breaker state
    pub async fn get_state(&self) -> CircuitBreakerState {
        self.state.read().await.state
    }

    /// Get circuit breaker statistics
    pub async fn get_statistics(&self) -> CircuitBreakerStatistics {
        let state = self.state.read().await;
        let now = Instant::now();

        CircuitBreakerStatistics {
            component_name: self.component_name.clone(),
            state: state.state,
            failure_count: state.failure_count,
            success_count: state.success_count,
            last_failure_time: state.last_failure_time,
            last_state_change: state.last_state_change,
            time_since_last_state_change: now.duration_since(state.last_state_change),
            half_open_requests: state.half_open_requests,
            config: self.config.clone(),
        }
    }

    /// Force the circuit breaker to a specific state (admin operation)
    pub async fn force_state(&self, new_state: CircuitBreakerState) -> AccessControlResult<()> {
        let mut state = self.state.write().await;
        let now = Instant::now();

        let old_state = state.state;
        state.state = new_state;
        state.last_state_change = now;

        match new_state {
            CircuitBreakerState::Closed => {
                state.failure_count = 0;
                state.success_count = 0;
                state.half_open_requests = 0;
                state.failure_times.clear();
            }
            CircuitBreakerState::HalfOpen => {
                state.success_count = 0;
                state.half_open_requests = 0;
            }
            CircuitBreakerState::Open => {
                // Keep current failure state
            }
        }

        info!(
            "Circuit breaker '{}': Manually changed state from {:?} to {:?}",
            self.component_name, old_state, new_state
        );

        Ok(())
    }

    /// Reset circuit breaker to initial state
    pub async fn reset(&self) -> AccessControlResult<()> {
        let mut state = self.state.write().await;
        *state = CircuitBreakerInternalState::default();

        info!(
            "Circuit breaker '{}': Reset to initial state",
            self.component_name
        );
        Ok(())
    }
}

/// Statistics for monitoring and alerting
#[derive(Debug, Clone, serde::Serialize)]
pub struct CircuitBreakerStatistics {
    pub component_name: String,
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    pub success_count: u32,
    #[serde(skip)]
    pub last_failure_time: Option<Instant>,
    #[serde(skip)]
    pub last_state_change: Instant,
    #[serde(serialize_with = "serialize_duration")]
    pub time_since_last_state_change: Duration,
    pub half_open_requests: u32,
    pub config: CircuitBreakerConfig,
}

impl serde::Serialize for CircuitBreakerState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            CircuitBreakerState::Closed => "closed",
            CircuitBreakerState::Open => "open",
            CircuitBreakerState::HalfOpen => "half_open",
        };
        serializer.serialize_str(s)
    }
}

fn serialize_duration<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_u64(duration.as_secs())
}

/// Health check result for circuit breaker
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthCheckResult {
    pub component_name: String,
    pub is_healthy: bool,
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    #[serde(skip)]
    pub last_check: Instant,
}

impl CircuitBreaker {
    /// Perform a health check on the protected component
    pub async fn health_check<F, Fut>(&self, check_fn: F) -> HealthCheckResult
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AccessControlResult<()>>,
    {
        let state = self.get_state().await;
        let statistics = self.get_statistics().await;

        let is_healthy = match state {
            CircuitBreakerState::Closed => {
                // Perform actual health check
                match check_fn().await {
                    Ok(_) => true,
                    Err(_) => false,
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Partially healthy - some requests allowed
                true
            }
            CircuitBreakerState::Open => {
                // Not healthy - circuit is open
                false
            }
        };

        HealthCheckResult {
            component_name: self.component_name.clone(),
            is_healthy,
            state,
            failure_count: statistics.failure_count,
            last_check: Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout: Duration::from_millis(100),
            success_threshold: 2,
            half_open_max_requests: 2,
            failure_window: Duration::from_secs(60),
        };

        let cb = CircuitBreaker::new("test".to_string(), config);

        // Should start closed
        assert_eq!(cb.get_state().await, CircuitBreakerState::Closed);

        // Simulate failures
        for i in 0..3 {
            let result = cb
                .execute(|| async {
                    use crate::errors::context::ErrorContext;
                    Err::<(), _>(AccessControlError::AbacError {
                        message: format!("failure {}", i),
                        source: None,
                        context: ErrorContext::new("test"),
                    })
                })
                .await;
            assert!(result.is_err());
        }

        // Should be open now
        assert_eq!(cb.get_state().await, CircuitBreakerState::Open);

        // Next request should fail fast
        let result = cb.execute(|| async { Ok::<(), _>(()) }).await;
        assert!(matches!(
            result,
            Err(AccessControlError::CircuitBreakerOpen { .. })
        ));
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_transition() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(50),
            success_threshold: 2,
            half_open_max_requests: 3,
            failure_window: Duration::from_secs(60),
        };

        let cb = CircuitBreaker::new("test".to_string(), config);

        // Trigger failures to open circuit
        for i in 0..2 {
            let _ = cb
                .execute(|| async {
                    use crate::errors::context::ErrorContext;
                    Err::<(), _>(AccessControlError::AbacError {
                        message: format!("failure {}", i),
                        source: None,
                        context: ErrorContext::new("test"),
                    })
                })
                .await;
        }

        assert_eq!(cb.get_state().await, CircuitBreakerState::Open);

        // Wait for timeout
        sleep(Duration::from_millis(60)).await;

        // Next request should transition to half-open
        let result = cb.execute(|| async { Ok::<(), _>(()) }).await;
        assert!(result.is_ok());
        assert_eq!(cb.get_state().await, CircuitBreakerState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closes_after_successes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(50),
            success_threshold: 2,
            half_open_max_requests: 5,
            failure_window: Duration::from_secs(60),
        };

        let cb = CircuitBreaker::new("test".to_string(), config);

        // Open the circuit
        for i in 0..2 {
            let _ = cb
                .execute(|| async {
                    use crate::errors::context::ErrorContext;
                    Err::<(), _>(AccessControlError::AbacError {
                        message: format!("failure {}", i),
                        source: None,
                        context: ErrorContext::new("test"),
                    })
                })
                .await;
        }

        // Wait and transition to half-open
        sleep(Duration::from_millis(60)).await;

        // Execute successful operations
        for _ in 0..2 {
            let result = cb.execute(|| async { Ok::<(), _>(()) }).await;
            assert!(result.is_ok());
        }

        // Should be closed now
        assert_eq!(cb.get_state().await, CircuitBreakerState::Closed);
    }
}
