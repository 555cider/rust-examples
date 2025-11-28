use super::context::ErrorContext;
use std::time::Duration;
use thiserror::Error;

/// Comprehensive error types for the hybrid access control system
#[derive(Error, Debug)]
pub enum AccessControlError {
    // RBAC-related errors
    #[error("RBAC evaluation failed: {message}")]
    RbacError {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
        context: ErrorContext,
    },

    #[error("Role not found: {role}")]
    RoleNotFound { role: String, context: ErrorContext },

    #[error("Permission not found: {permission}")]
    PermissionNotFound {
        permission: String,
        context: ErrorContext,
    },

    #[error("Invalid role hierarchy: {message}")]
    InvalidRoleHierarchy {
        message: String,
        context: ErrorContext,
    },

    // ABAC-related errors
    #[error("ABAC evaluation failed: {message}")]
    AbacError {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
        context: ErrorContext,
    },

    #[error("Policy parsing error: {message}")]
    PolicyParsingError {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
        context: ErrorContext,
    },

    #[error("Invalid policy configuration: {message}")]
    InvalidPolicyConfig {
        message: String,
        context: ErrorContext,
    },

    #[error("Policy not found: {policy_id}")]
    PolicyNotFound {
        policy_id: String,
        context: ErrorContext,
    },

    #[error("Policy evaluation timeout")]
    PolicyEvaluationTimeout { context: ErrorContext },

    // Attribute-related errors
    #[error("Attribute not found: {attribute_name} for {entity_type} {entity_id}")]
    AttributeNotFound {
        attribute_name: String,
        entity_type: String,
        entity_id: String,
        context: ErrorContext,
    },

    #[error("Invalid attribute value: {attribute_name} = {value}")]
    InvalidAttributeValue {
        attribute_name: String,
        value: String,
        context: ErrorContext,
    },

    #[error("Attribute type mismatch: expected {expected}, got {actual}")]
    AttributeTypeMismatch {
        expected: String,
        actual: String,
        context: ErrorContext,
    },

    // Evaluation errors
    #[error("Access denied: {reason}")]
    AccessDenied {
        reason: String,
        context: ErrorContext,
    },

    #[error("Evaluation timeout after {duration:?}")]
    EvaluationTimeout {
        duration: Duration,
        context: ErrorContext,
    },

    #[error("Conflicting evaluation results: RBAC={rbac_result}, ABAC={abac_result}")]
    ConflictingResults {
        rbac_result: String,
        abac_result: String,
        context: ErrorContext,
    },

    #[error("No applicable policies found for resource: {resource_type}:{resource_id}")]
    NoApplicablePolicies {
        resource_type: String,
        resource_id: String,
        context: ErrorContext,
    },

    // System errors
    #[error("Configuration error: {message}")]
    ConfigurationError {
        message: String,
        context: ErrorContext,
    },

    #[error("Cache error: {operation}")]
    CacheError {
        operation: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
        context: ErrorContext,
    },

    #[error("Database error: {operation}")]
    DatabaseError {
        operation: String,
        #[source]
        source: sqlx::Error,
        context: ErrorContext,
    },

    #[error("Serialization error: {message}")]
    SerializationError {
        message: String,
        #[source]
        source: serde_json::Error,
        context: ErrorContext,
    },

    #[error("Network error: {message}")]
    NetworkError {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
        context: ErrorContext,
    },

    // Circuit breaker errors
    #[error("Circuit breaker is open for {component}")]
    CircuitBreakerOpen {
        component: String,
        context: ErrorContext,
    },

    #[error("Circuit breaker half-open, limited requests allowed for {component}")]
    CircuitBreakerHalfOpen {
        component: String,
        context: ErrorContext,
    },

    // Recovery and fallback errors
    #[error("Fallback mechanism failed: {message}")]
    FallbackFailed {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
        context: ErrorContext,
    },

    #[error("Graceful degradation not available: {message}")]
    GracefulDegradationUnavailable {
        message: String,
        context: ErrorContext,
    },

    // Validation errors
    #[error("Invalid access request: {message}")]
    InvalidAccessRequest {
        message: String,
        context: ErrorContext,
    },

    #[error("Missing required context: {field}")]
    MissingRequiredContext {
        field: String,
        context: ErrorContext,
    },

    // Performance errors
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted {
        resource: String,
        context: ErrorContext,
    },

    #[error("Rate limit exceeded for {entity}")]
    RateLimitExceeded {
        entity: String,
        context: ErrorContext,
    },
}

/// Error severity levels for monitoring and alerting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Low,      // Expected errors, normal operation
    Medium,   // Unexpected but recoverable errors
    High,     // System errors requiring attention
    Critical, // System failures requiring immediate action
}

impl AccessControlError {
    /// Log the error with structured fields and context
    pub fn log_error(&self) {
        use tracing::{error, info, warn};

        let context = self.context();
        let severity = self.severity();
        let error_code = self.error_code();

        // Redact sensitive information from context
        let redacted_context = {
            let mut ctx_json = serde_json::to_value(context).unwrap_or(serde_json::Value::Null);
            let redactor = crate::utils::log_redaction::LogRedactor::new();
            redactor.redact_json(&mut ctx_json);
            ctx_json
        };

        match severity {
            ErrorSeverity::Critical => {
                error!(
                    error_code = error_code,
                    error_message = %self,
                    operation = %context.operation,
                    user_id = ?context.user_id,
                    resource = ?context.resource,
                    action = ?context.action,
                    context = ?redacted_context,
                    severity = "CRITICAL",
                    "Critical access control error occurred"
                );
            }
            ErrorSeverity::High => {
                error!(
                    error_code = error_code,
                    error_message = %self,
                    operation = %context.operation,
                    user_id = ?context.user_id,
                    resource = ?context.resource,
                    action = ?context.action,
                    context = ?redacted_context,
                    severity = "HIGH",
                    "High severity access control error"
                );
            }
            ErrorSeverity::Medium => {
                warn!(
                    error_code = error_code,
                    error_message = %self,
                    operation = %context.operation,
                    user_id = ?context.user_id,
                    resource = ?context.resource,
                    action = ?context.action,
                    context = ?redacted_context,
                    severity = "MEDIUM",
                    "Medium severity access control error"
                );
            }
            ErrorSeverity::Low => {
                info!(
                    error_code = error_code,
                    error_message = %self,
                    operation = %context.operation,
                    user_id = ?context.user_id,
                    resource = ?context.resource,
                    action = ?context.action,
                    severity = "LOW",
                    "Access control decision"
                );
            }
        }
    }

    /// Get the error context
    pub fn context(&self) -> &ErrorContext {
        match self {
            AccessControlError::RbacError { context, .. } => context,
            AccessControlError::RoleNotFound { context, .. } => context,
            AccessControlError::PermissionNotFound { context, .. } => context,
            AccessControlError::InvalidRoleHierarchy { context, .. } => context,
            AccessControlError::AbacError { context, .. } => context,
            AccessControlError::PolicyParsingError { context, .. } => context,
            AccessControlError::InvalidPolicyConfig { context, .. } => context,
            AccessControlError::PolicyNotFound { context, .. } => context,
            AccessControlError::PolicyEvaluationTimeout { context, .. } => context,
            AccessControlError::AttributeNotFound { context, .. } => context,
            AccessControlError::InvalidAttributeValue { context, .. } => context,
            AccessControlError::AttributeTypeMismatch { context, .. } => context,
            AccessControlError::AccessDenied { context, .. } => context,
            AccessControlError::EvaluationTimeout { context, .. } => context,
            AccessControlError::ConflictingResults { context, .. } => context,
            AccessControlError::NoApplicablePolicies { context, .. } => context,
            AccessControlError::ConfigurationError { context, .. } => context,
            AccessControlError::CacheError { context, .. } => context,
            AccessControlError::DatabaseError { context, .. } => context,
            AccessControlError::SerializationError { context, .. } => context,
            AccessControlError::NetworkError { context, .. } => context,
            AccessControlError::CircuitBreakerOpen { context, .. } => context,
            AccessControlError::CircuitBreakerHalfOpen { context, .. } => context,
            AccessControlError::FallbackFailed { context, .. } => context,
            AccessControlError::GracefulDegradationUnavailable { context, .. } => context,
            AccessControlError::InvalidAccessRequest { context, .. } => context,
            AccessControlError::MissingRequiredContext { context, .. } => context,
            AccessControlError::ResourceExhausted { context, .. } => context,
            AccessControlError::RateLimitExceeded { context, .. } => context,
        }
    }

    /// Get the severity level of the error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            // Low severity - expected during normal operation
            AccessControlError::AccessDenied { .. } => ErrorSeverity::Low,
            AccessControlError::AttributeNotFound { .. } => ErrorSeverity::Low,
            AccessControlError::NoApplicablePolicies { .. } => ErrorSeverity::Low,
            AccessControlError::RoleNotFound { .. } => ErrorSeverity::Low,
            AccessControlError::PermissionNotFound { .. } => ErrorSeverity::Low,
            AccessControlError::PolicyNotFound { .. } => ErrorSeverity::Low,

            // Medium severity - unexpected but recoverable
            AccessControlError::PolicyParsingError { .. } => ErrorSeverity::Medium,
            AccessControlError::InvalidPolicyConfig { .. } => ErrorSeverity::Medium,
            AccessControlError::InvalidAttributeValue { .. } => ErrorSeverity::Medium,
            AccessControlError::AttributeTypeMismatch { .. } => ErrorSeverity::Medium,
            AccessControlError::ConflictingResults { .. } => ErrorSeverity::Medium,
            AccessControlError::InvalidAccessRequest { .. } => ErrorSeverity::Medium,
            AccessControlError::MissingRequiredContext { .. } => ErrorSeverity::Medium,
            AccessControlError::CacheError { .. } => ErrorSeverity::Medium,

            // High severity - system errors requiring attention
            AccessControlError::RbacError { .. } => ErrorSeverity::High,
            AccessControlError::AbacError { .. } => ErrorSeverity::High,
            AccessControlError::EvaluationTimeout { .. } => ErrorSeverity::High,
            AccessControlError::PolicyEvaluationTimeout { .. } => ErrorSeverity::High,
            AccessControlError::DatabaseError { .. } => ErrorSeverity::High,
            AccessControlError::NetworkError { .. } => ErrorSeverity::High,
            AccessControlError::SerializationError { .. } => ErrorSeverity::High,
            AccessControlError::ResourceExhausted { .. } => ErrorSeverity::High,
            AccessControlError::RateLimitExceeded { .. } => ErrorSeverity::High,

            // Critical severity - system failures
            AccessControlError::ConfigurationError { .. } => ErrorSeverity::Critical,
            AccessControlError::InvalidRoleHierarchy { .. } => ErrorSeverity::Critical,
            AccessControlError::CircuitBreakerOpen { .. } => ErrorSeverity::Critical,
            AccessControlError::CircuitBreakerHalfOpen { .. } => ErrorSeverity::Critical,
            AccessControlError::FallbackFailed { .. } => ErrorSeverity::Critical,
            AccessControlError::GracefulDegradationUnavailable { .. } => ErrorSeverity::Critical,
        }
    }

    /// Check if the error is recoverable through fallback mechanisms
    pub fn is_recoverable(&self) -> bool {
        match self {
            // ABAC failures can fallback to RBAC
            AccessControlError::AbacError { .. } => true,
            AccessControlError::PolicyParsingError { .. } => true,
            AccessControlError::PolicyEvaluationTimeout { .. } => true,
            AccessControlError::AttributeNotFound { .. } => true,
            AccessControlError::InvalidAttributeValue { .. } => true,
            AccessControlError::NoApplicablePolicies { .. } => true,

            // Cache errors can fallback to direct database access
            AccessControlError::CacheError { .. } => true,

            // Network errors might be temporary
            AccessControlError::NetworkError { .. } => true,

            // These errors are not recoverable
            AccessControlError::RbacError { .. } => false,
            AccessControlError::DatabaseError { .. } => false,
            AccessControlError::ConfigurationError { .. } => false,
            AccessControlError::InvalidRoleHierarchy { .. } => false,
            AccessControlError::CircuitBreakerOpen { .. } => false,
            AccessControlError::FallbackFailed { .. } => false,

            // Other errors might be recoverable depending on context
            _ => false,
        }
    }

    /// Get error code for API responses
    pub fn error_code(&self) -> &'static str {
        match self {
            AccessControlError::RbacError { .. } => "RBAC_ERROR",
            AccessControlError::RoleNotFound { .. } => "ROLE_NOT_FOUND",
            AccessControlError::PermissionNotFound { .. } => "PERMISSION_NOT_FOUND",
            AccessControlError::InvalidRoleHierarchy { .. } => "INVALID_ROLE_HIERARCHY",
            AccessControlError::AbacError { .. } => "ABAC_ERROR",
            AccessControlError::PolicyParsingError { .. } => "POLICY_PARSING_ERROR",
            AccessControlError::InvalidPolicyConfig { .. } => "INVALID_POLICY_CONFIG",
            AccessControlError::PolicyNotFound { .. } => "POLICY_NOT_FOUND",
            AccessControlError::PolicyEvaluationTimeout { .. } => "POLICY_EVALUATION_TIMEOUT",
            AccessControlError::AttributeNotFound { .. } => "ATTRIBUTE_NOT_FOUND",
            AccessControlError::InvalidAttributeValue { .. } => "INVALID_ATTRIBUTE_VALUE",
            AccessControlError::AttributeTypeMismatch { .. } => "ATTRIBUTE_TYPE_MISMATCH",
            AccessControlError::AccessDenied { .. } => "ACCESS_DENIED",
            AccessControlError::EvaluationTimeout { .. } => "EVALUATION_TIMEOUT",
            AccessControlError::ConflictingResults { .. } => "CONFLICTING_RESULTS",
            AccessControlError::NoApplicablePolicies { .. } => "NO_APPLICABLE_POLICIES",
            AccessControlError::ConfigurationError { .. } => "CONFIGURATION_ERROR",
            AccessControlError::CacheError { .. } => "CACHE_ERROR",
            AccessControlError::DatabaseError { .. } => "DATABASE_ERROR",
            AccessControlError::SerializationError { .. } => "SERIALIZATION_ERROR",
            AccessControlError::NetworkError { .. } => "NETWORK_ERROR",
            AccessControlError::CircuitBreakerOpen { .. } => "CIRCUIT_BREAKER_OPEN",
            AccessControlError::CircuitBreakerHalfOpen { .. } => "CIRCUIT_BREAKER_HALF_OPEN",
            AccessControlError::FallbackFailed { .. } => "FALLBACK_FAILED",
            AccessControlError::GracefulDegradationUnavailable { .. } => {
                "GRACEFUL_DEGRADATION_UNAVAILABLE"
            }
            AccessControlError::InvalidAccessRequest { .. } => "INVALID_ACCESS_REQUEST",
            AccessControlError::MissingRequiredContext { .. } => "MISSING_REQUIRED_CONTEXT",
            AccessControlError::ResourceExhausted { .. } => "RESOURCE_EXHAUSTED",
            AccessControlError::RateLimitExceeded { .. } => "RATE_LIMIT_EXCEEDED",
        }
    }
}

/// Convert from common error types
/// Note: These conversions create a default context. For better diagnostics,
/// prefer creating errors explicitly with proper context.
impl From<sqlx::Error> for AccessControlError {
    fn from(err: sqlx::Error) -> Self {
        AccessControlError::DatabaseError {
            operation: "unknown_database_operation".to_string(),
            source: err,
            context: ErrorContext::new("database_operation"),
        }
    }
}

impl From<serde_json::Error> for AccessControlError {
    fn from(err: serde_json::Error) -> Self {
        AccessControlError::SerializationError {
            message: err.to_string(),
            source: err,
            context: ErrorContext::new("serialization"),
        }
    }
}

impl From<tokio::time::error::Elapsed> for AccessControlError {
    fn from(_err: tokio::time::error::Elapsed) -> Self {
        AccessControlError::EvaluationTimeout {
            duration: Duration::from_millis(100), // Default timeout
            context: ErrorContext::new("evaluation_timeout"),
        }
    }
}

/// Result type for access control operations
pub type AccessControlResult<T> = Result<T, AccessControlError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_severity_classification() {
        let ctx = ErrorContext::new("test");

        assert_eq!(
            AccessControlError::AccessDenied {
                reason: "test".to_string(),
                context: ctx.clone(),
            }
            .severity(),
            ErrorSeverity::Low
        );

        assert_eq!(
            AccessControlError::PolicyParsingError {
                message: "test".to_string(),
                source: None,
                context: ctx.clone(),
            }
            .severity(),
            ErrorSeverity::Medium
        );

        assert_eq!(
            AccessControlError::DatabaseError {
                operation: "test".to_string(),
                source: sqlx::Error::RowNotFound,
                context: ctx.clone(),
            }
            .severity(),
            ErrorSeverity::High
        );

        assert_eq!(
            AccessControlError::ConfigurationError {
                message: "test".to_string(),
                context: ctx.clone(),
            }
            .severity(),
            ErrorSeverity::Critical
        );
    }

    #[test]
    fn test_error_recoverability() {
        let ctx = ErrorContext::new("test");

        assert!(AccessControlError::AbacError {
            message: "test".to_string(),
            source: None,
            context: ctx.clone(),
        }
        .is_recoverable());

        assert!(!AccessControlError::RbacError {
            message: "test".to_string(),
            source: None,
            context: ctx.clone(),
        }
        .is_recoverable());

        assert!(!AccessControlError::DatabaseError {
            operation: "test".to_string(),
            source: sqlx::Error::RowNotFound,
            context: ctx.clone(),
        }
        .is_recoverable());
    }

    #[test]
    fn test_error_codes() {
        let ctx = ErrorContext::new("test");

        assert_eq!(
            AccessControlError::AccessDenied {
                reason: "test".to_string(),
                context: ctx.clone(),
            }
            .error_code(),
            "ACCESS_DENIED"
        );

        assert_eq!(
            AccessControlError::PolicyNotFound {
                policy_id: "test".to_string(),
                context: ctx.clone(),
            }
            .error_code(),
            "POLICY_NOT_FOUND"
        );
    }

    #[test]
    fn test_error_context_access() {
        let ctx = ErrorContext::new("test_operation")
            .with_user(123)
            .with_resource("document:456");

        let error = AccessControlError::AccessDenied {
            reason: "insufficient permissions".to_string(),
            context: ctx.clone(),
        };

        assert_eq!(error.context().operation, "test_operation");
        assert_eq!(error.context().user_id, Some(123));
        assert_eq!(error.context().resource, Some("document:456".to_string()));
    }
}
