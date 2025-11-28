use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Error context for detailed diagnostics
///
/// This structure captures contextual information about where and why an error occurred,
/// enabling better debugging and monitoring in production environments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    /// The operation being performed when the error occurred
    pub operation: String,

    /// The user ID associated with the operation, if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i64>,

    /// The resource being accessed, if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,

    /// The action being performed on the resource, if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,

    /// Timestamp when the error occurred
    pub timestamp: DateTime<Utc>,

    /// Additional contextual data as key-value pairs
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub additional_data: HashMap<String, serde_json::Value>,
}

impl ErrorContext {
    /// Create a new ErrorContext with the specified operation
    ///
    /// # Arguments
    /// * `operation` - A description of the operation being performed
    ///
    /// # Example
    /// ```
    /// use admin_server::errors::context::ErrorContext;
    ///
    /// let context = ErrorContext::new("evaluate_permission");
    /// ```
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            user_id: None,
            resource: None,
            action: None,
            timestamp: Utc::now(),
            additional_data: HashMap::new(),
        }
    }

    /// Add user ID to the context (builder pattern)
    ///
    /// # Arguments
    /// * `user_id` - The ID of the user associated with the operation
    ///
    /// # Example
    /// ```
    /// use admin_server::errors::context::ErrorContext;
    ///
    /// let context = ErrorContext::new("evaluate_permission")
    ///     .with_user(123);
    /// ```
    pub fn with_user(mut self, user_id: i64) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Add resource to the context (builder pattern)
    ///
    /// # Arguments
    /// * `resource` - The resource being accessed
    ///
    /// # Example
    /// ```
    /// use admin_server::errors::context::ErrorContext;
    ///
    /// let context = ErrorContext::new("evaluate_permission")
    ///     .with_resource("document:123");
    /// ```
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    /// Add action to the context (builder pattern)
    ///
    /// # Arguments
    /// * `action` - The action being performed
    ///
    /// # Example
    /// ```
    /// use admin_server::errors::context::ErrorContext;
    ///
    /// let context = ErrorContext::new("evaluate_permission")
    ///     .with_action("read");
    /// ```
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// Add additional data to the context (builder pattern)
    ///
    /// # Arguments
    /// * `key` - The key for the additional data
    /// * `value` - The value to store (must be serializable)
    ///
    /// # Example
    /// ```
    /// use admin_server::errors::context::ErrorContext;
    ///
    /// let context = ErrorContext::new("evaluate_permission")
    ///     .with_data("policy_id", 456)
    ///     .with_data("evaluation_mode", "strict");
    /// ```
    pub fn with_data<T: Serialize>(mut self, key: impl Into<String>, value: T) -> Self {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.additional_data.insert(key.into(), json_value);
        }
        self
    }

    /// Get a reference to additional data by key
    pub fn get_data(&self, key: &str) -> Option<&serde_json::Value> {
        self.additional_data.get(key)
    }

    /// Check if the context has a specific user ID
    pub fn has_user(&self) -> bool {
        self.user_id.is_some()
    }

    /// Check if the context has a resource
    pub fn has_resource(&self) -> bool {
        self.resource.is_some()
    }

    /// Check if the context has an action
    pub fn has_action(&self) -> bool {
        self.action.is_some()
    }
}

impl Default for ErrorContext {
    fn default() -> Self {
        Self::new("unknown_operation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context_creation() {
        let context = ErrorContext::new("test_operation");
        assert_eq!(context.operation, "test_operation");
        assert!(context.user_id.is_none());
        assert!(context.resource.is_none());
        assert!(context.action.is_none());
        assert!(context.additional_data.is_empty());
    }

    #[test]
    fn test_error_context_builder_pattern() {
        let context = ErrorContext::new("evaluate_permission")
            .with_user(123)
            .with_resource("document:456")
            .with_action("read")
            .with_data("policy_id", 789)
            .with_data("evaluation_mode", "strict");

        assert_eq!(context.operation, "evaluate_permission");
        assert_eq!(context.user_id, Some(123));
        assert_eq!(context.resource, Some("document:456".to_string()));
        assert_eq!(context.action, Some("read".to_string()));
        assert_eq!(context.additional_data.len(), 2);
        assert!(context.get_data("policy_id").is_some());
        assert!(context.get_data("evaluation_mode").is_some());
    }

    #[test]
    fn test_error_context_has_methods() {
        let context = ErrorContext::new("test")
            .with_user(1)
            .with_resource("test_resource");

        assert!(context.has_user());
        assert!(context.has_resource());
        assert!(!context.has_action());
    }

    #[test]
    fn test_error_context_serialization() {
        let context = ErrorContext::new("test_operation")
            .with_user(123)
            .with_resource("document:456")
            .with_action("read");

        let json = serde_json::to_string(&context).unwrap();
        assert!(json.contains("test_operation"));
        assert!(json.contains("123"));
        assert!(json.contains("document:456"));
        assert!(json.contains("read"));
    }

    #[test]
    fn test_error_context_default() {
        let context = ErrorContext::default();
        assert_eq!(context.operation, "unknown_operation");
    }
}
