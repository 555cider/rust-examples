use regex::Regex;
use serde::{Deserialize, Serialize};
use std::env;

/// Security configuration for access control system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Super admin role identifiers
    pub super_admin_roles: Vec<String>,

    /// Super admin role IDs
    pub super_admin_role_ids: Vec<i64>,

    /// Require MFA for admin operations
    pub require_mfa_for_admin: bool,

    /// Allowed permission code patterns (regex)
    pub allowed_permission_patterns: Vec<String>,

    /// Sensitive fields to redact in logs
    pub log_redaction_fields: Vec<String>,
}

impl SecurityConfig {
    /// Load security configuration from environment variables
    pub fn from_env() -> Self {
        let super_admin_roles = env::var("SUPER_ADMIN_ROLES")
            .unwrap_or_else(|_| "super_admin,system_admin".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let super_admin_role_ids = env::var("SUPER_ADMIN_ROLE_IDS")
            .unwrap_or_else(|_| "1".to_string())
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        let require_mfa_for_admin = env::var("REQUIRE_MFA_FOR_ADMIN")
            .unwrap_or_else(|_| "false".to_string())
            .parse()
            .unwrap_or(false);

        let allowed_permission_patterns = env::var("ALLOWED_PERMISSION_PATTERNS")
            .unwrap_or_else(|_| "^[a-z_]+[:.][a-z_]+$".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let log_redaction_fields = env::var("LOG_REDACTION_FIELDS")
            .unwrap_or_else(|_| "password,token,secret,api_key,auth_token".to_string())
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            super_admin_roles,
            super_admin_role_ids,
            require_mfa_for_admin,
            allowed_permission_patterns,
            log_redaction_fields,
        }
    }

    /// Check if a role name is a super admin role
    pub fn is_super_admin_role(&self, role_name: &str) -> bool {
        self.super_admin_roles
            .iter()
            .any(|r| r.eq_ignore_ascii_case(role_name))
    }

    /// Check if a role ID is a super admin role
    pub fn is_super_admin_role_id(&self, role_id: i64) -> bool {
        self.super_admin_role_ids.contains(&role_id)
    }

    /// Validate a permission code against allowed patterns
    pub fn validate_permission_code(&self, code: &str) -> Result<(), String> {
        if self.allowed_permission_patterns.is_empty() {
            return Ok(());
        }

        for pattern in &self.allowed_permission_patterns {
            match Regex::new(pattern) {
                Ok(regex) => {
                    if regex.is_match(code) {
                        return Ok(());
                    }
                }
                Err(e) => {
                    tracing::warn!("Invalid regex pattern '{}': {}", pattern, e);
                    continue;
                }
            }
        }

        Err(format!(
            "Permission code '{}' does not match any allowed patterns",
            code
        ))
    }

    /// Check if a field name should be redacted in logs
    pub fn should_redact_field(&self, field_name: &str) -> bool {
        let field_lower = field_name.to_lowercase();
        self.log_redaction_fields
            .iter()
            .any(|f| field_lower.contains(f))
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            super_admin_roles: vec!["super_admin".to_string(), "system_admin".to_string()],
            super_admin_role_ids: vec![1],
            require_mfa_for_admin: false,
            allowed_permission_patterns: vec!["^[a-z_]+[:.][a-z_]+$".to_string()],
            log_redaction_fields: vec![
                "password".to_string(),
                "token".to_string(),
                "secret".to_string(),
                "api_key".to_string(),
                "auth_token".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_super_admin_role() {
        let config = SecurityConfig::default();

        assert!(config.is_super_admin_role("super_admin"));
        assert!(config.is_super_admin_role("SUPER_ADMIN"));
        assert!(config.is_super_admin_role("system_admin"));
        assert!(!config.is_super_admin_role("regular_admin"));
        assert!(!config.is_super_admin_role("user"));
    }

    #[test]
    fn test_is_super_admin_role_id() {
        let config = SecurityConfig::default();

        assert!(config.is_super_admin_role_id(1));
        assert!(!config.is_super_admin_role_id(2));
        assert!(!config.is_super_admin_role_id(100));
    }

    #[test]
    fn test_validate_permission_code() {
        let config = SecurityConfig::default();

        // Valid codes (both dot and colon separators)
        assert!(config.validate_permission_code("user.create").is_ok());
        assert!(config.validate_permission_code("user:create").is_ok());
        assert!(config.validate_permission_code("permission.delete").is_ok());
        assert!(config.validate_permission_code("permission:delete").is_ok());
        assert!(config.validate_permission_code("role.update").is_ok());
        assert!(config.validate_permission_code("role:update").is_ok());

        // Invalid codes
        assert!(config.validate_permission_code("UserCreate").is_err());
        assert!(config.validate_permission_code("user-create").is_err());
        assert!(config.validate_permission_code("user").is_err());
        assert!(config
            .validate_permission_code("user.create.extra")
            .is_err());
    }

    #[test]
    fn test_should_redact_field() {
        let config = SecurityConfig::default();

        assert!(config.should_redact_field("password"));
        assert!(config.should_redact_field("user_password"));
        assert!(config.should_redact_field("PASSWORD"));
        assert!(config.should_redact_field("auth_token"));
        assert!(config.should_redact_field("api_key"));
        assert!(config.should_redact_field("secret_key"));

        assert!(!config.should_redact_field("username"));
        assert!(!config.should_redact_field("email"));
        assert!(!config.should_redact_field("user_id"));
    }

    #[test]
    fn test_validate_permission_code_with_multiple_patterns() {
        let config = SecurityConfig {
            allowed_permission_patterns: vec![
                "^[a-z_]+[:.][a-z_]+$".to_string(),
                "^admin[:.][a-z_]+[:.][a-z_]+$".to_string(),
            ],
            ..Default::default()
        };

        // Valid for first pattern
        assert!(config.validate_permission_code("user.create").is_ok());
        assert!(config.validate_permission_code("user:create").is_ok());

        // Valid for second pattern
        assert!(config.validate_permission_code("admin.user.create").is_ok());
        assert!(config.validate_permission_code("admin:user:create").is_ok());

        // Invalid for both patterns
        assert!(config.validate_permission_code("invalid").is_err());
    }

    #[test]
    fn test_validate_permission_code_empty_patterns() {
        let config = SecurityConfig {
            allowed_permission_patterns: vec![],
            ..Default::default()
        };

        // Should allow any code when no patterns are configured
        assert!(config.validate_permission_code("anything").is_ok());
        assert!(config.validate_permission_code("user.create").is_ok());
        assert!(config.validate_permission_code("INVALID-CODE").is_ok());
    }
}
