use crate::config::security::SecurityConfig;
use serde_json::Value;
use std::collections::HashSet;

/// Sensitive field names that should be redacted in logs
const DEFAULT_SENSITIVE_FIELDS: &[&str] = &[
    "password",
    "password_hash",
    "token",
    "secret",
    "api_key",
    "access_token",
    "refresh_token",
    "authorization",
    "cookie",
    "session",
    "credit_card",
    "ssn",
    "social_security",
];

/// Log redaction utility for removing sensitive information from logs
#[derive(Clone)]
pub struct LogRedactor {
    sensitive_fields: HashSet<String>,
}

impl LogRedactor {
    /// Create a new log redactor with default sensitive fields
    pub fn new() -> Self {
        Self {
            sensitive_fields: DEFAULT_SENSITIVE_FIELDS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    /// Create a log redactor from SecurityConfig
    pub fn from_security_config(config: &SecurityConfig) -> Self {
        let mut sensitive_fields: HashSet<String> = DEFAULT_SENSITIVE_FIELDS
            .iter()
            .map(|s| s.to_string())
            .collect();

        for field in &config.log_redaction_fields {
            sensitive_fields.insert(field.to_lowercase());
        }

        Self { sensitive_fields }
    }

    /// Create a log redactor with custom sensitive fields
    pub fn with_fields(fields: Vec<String>) -> Self {
        let mut sensitive_fields: HashSet<String> = DEFAULT_SENSITIVE_FIELDS
            .iter()
            .map(|s| s.to_string())
            .collect();

        for field in fields {
            sensitive_fields.insert(field.to_lowercase());
        }

        Self { sensitive_fields }
    }

    /// Add a sensitive field to the redactor
    pub fn add_field(&mut self, field: impl Into<String>) {
        self.sensitive_fields.insert(field.into().to_lowercase());
    }

    /// Check if a field name is sensitive
    pub fn is_sensitive(&self, field_name: &str) -> bool {
        let field_lower = field_name.to_lowercase();

        // Check exact match
        if self.sensitive_fields.contains(&field_lower) {
            return true;
        }

        // Check if field contains any sensitive keyword
        for sensitive in &self.sensitive_fields {
            if field_lower.contains(sensitive) {
                return true;
            }
        }

        false
    }

    /// Redact sensitive information from a JSON value
    pub fn redact_json(&self, value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (key, val) in map.iter_mut() {
                    if self.is_sensitive(key) {
                        *val = Value::String("[REDACTED]".to_string());
                    } else {
                        self.redact_json(val);
                    }
                }
            }
            Value::Array(arr) => {
                for item in arr.iter_mut() {
                    self.redact_json(item);
                }
            }
            _ => {}
        }
    }

    /// Redact sensitive information from a string (simple pattern matching)
    pub fn redact_string(&self, text: &str) -> String {
        let mut result = text.to_string();

        // Redact common patterns
        // Email addresses
        result = regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b")
            .unwrap()
            .replace_all(&result, "[EMAIL_REDACTED]")
            .to_string();

        // Credit card numbers (simple pattern)
        result = regex::Regex::new(r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b")
            .unwrap()
            .replace_all(&result, "[CARD_REDACTED]")
            .to_string();

        // JWT tokens (Bearer tokens)
        result = regex::Regex::new(r"Bearer\s+[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+")
            .unwrap()
            .replace_all(&result, "Bearer [TOKEN_REDACTED]")
            .to_string();

        result
    }

    /// Redact sensitive fields from a serializable struct
    pub fn redact_struct<T: serde::Serialize>(&self, data: &T) -> Value {
        let mut value = serde_json::to_value(data).unwrap_or(Value::Null);
        self.redact_json(&mut value);
        value
    }
}

impl Default for LogRedactor {
    fn default() -> Self {
        Self::new()
    }
}

use std::sync::OnceLock;

/// Global log redactor instance
static GLOBAL_REDACTOR: OnceLock<LogRedactor> = OnceLock::new();

/// Initialize the global log redactor with SecurityConfig
pub fn init_global_redactor(config: &SecurityConfig) {
    let _ = GLOBAL_REDACTOR.set(LogRedactor::from_security_config(config));
}

/// Get the global log redactor instance
pub fn global_redactor() -> &'static LogRedactor {
    GLOBAL_REDACTOR.get_or_init(|| {
        // Fallback to default if not initialized
        tracing::warn!("Global log redactor not initialized, using defaults");
        LogRedactor::new()
    })
}

/// Helper function to create a redacted JSON value for logging
pub fn redact_for_logging<T: serde::Serialize>(data: &T) -> Value {
    global_redactor().redact_struct(data)
}

/// Helper function to redact a string for logging
pub fn redact_string_for_logging(text: &str) -> String {
    global_redactor().redact_string(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_is_sensitive() {
        let redactor = LogRedactor::new();

        assert!(redactor.is_sensitive("password"));
        assert!(redactor.is_sensitive("Password"));
        assert!(redactor.is_sensitive("user_password"));
        assert!(redactor.is_sensitive("token"));
        assert!(redactor.is_sensitive("access_token"));
        assert!(!redactor.is_sensitive("username"));
        assert!(!redactor.is_sensitive("email"));
    }

    #[test]
    fn test_redact_json() {
        let redactor = LogRedactor::new();
        let mut data = json!({
            "username": "john_doe",
            "password": "secret123",
            "email": "john@example.com",
            "token": "abc123xyz",
            "profile": {
                "name": "John Doe",
                "api_key": "key123"
            }
        });

        redactor.redact_json(&mut data);

        assert_eq!(data["username"], "john_doe");
        assert_eq!(data["password"], "[REDACTED]");
        assert_eq!(data["email"], "john@example.com");
        assert_eq!(data["token"], "[REDACTED]");
        assert_eq!(data["profile"]["name"], "John Doe");
        assert_eq!(data["profile"]["api_key"], "[REDACTED]");
    }

    #[test]
    fn test_redact_string() {
        let redactor = LogRedactor::new();

        let text = "User email is john@example.com and token is Bearer eyJhbGc.eyJzdWI.SflKxw";
        let redacted = redactor.redact_string(text);

        assert!(redacted.contains("[EMAIL_REDACTED]"));
        assert!(redacted.contains("[TOKEN_REDACTED]"));
        assert!(!redacted.contains("john@example.com"));
    }

    #[test]
    fn test_custom_fields() {
        let mut redactor = LogRedactor::with_fields(vec!["custom_secret".to_string()]);
        redactor.add_field("another_secret");

        assert!(redactor.is_sensitive("custom_secret"));
        assert!(redactor.is_sensitive("another_secret"));
        assert!(redactor.is_sensitive("password")); // Default field
    }

    #[test]
    fn test_redact_nested_arrays() {
        let redactor = LogRedactor::new();
        let mut data = json!({
            "users": [
                {"username": "user1", "password": "pass1"},
                {"username": "user2", "password": "pass2"}
            ]
        });

        redactor.redact_json(&mut data);

        assert_eq!(data["users"][0]["password"], "[REDACTED]");
        assert_eq!(data["users"][1]["password"], "[REDACTED]");
        assert_eq!(data["users"][0]["username"], "user1");
    }
}
