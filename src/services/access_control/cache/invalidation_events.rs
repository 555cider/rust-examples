use serde::{Deserialize, Serialize};

/// Cache invalidation event types
///
/// These events trigger cache invalidation when data changes occur in the system.
/// Each event type corresponds to a specific data change that affects cached access control decisions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InvalidationEvent {
    // ===== User-related events =====
    /// User record was updated (profile, settings, etc.)
    UserUpdated { user_id: i64 },

    /// User was deleted from the system
    UserDeleted { user_id: i64 },

    /// User's permissions were explicitly changed
    /// This is the primary event for permission changes
    UserPermissionsChanged { user_id: i64 },

    /// User's attributes changed (department, location, etc.)
    /// Affects ABAC policy evaluation
    UserAttributesChanged { user_id: i64 },

    // ===== Role-related events =====
    /// New role was created
    RoleCreated { role_id: String },

    /// Role definition was updated
    RoleUpdated { role_id: String },

    /// Role was deleted
    RoleDeleted { role_id: String },

    /// Permissions assigned to a role were changed
    /// This affects all users with this role
    RolePermissionsChanged { role_id: String },

    // ===== Policy-related events =====
    /// New ABAC policy was created
    PolicyCreated { policy_id: String },

    /// ABAC policy was updated
    /// This affects all evaluations matching the policy target
    PolicyUpdated { policy_id: String },

    /// ABAC policy was deleted
    PolicyDeleted { policy_id: String },

    /// Policy was activated (enabled)
    PolicyActivated { policy_id: String },

    /// Policy was deactivated (disabled)
    PolicyDeactivated { policy_id: String },

    // ===== Resource-related events =====
    /// New resource was created
    ResourceCreated {
        resource_id: String,
        resource_type: String,
    },

    /// Resource was updated
    ResourceUpdated {
        resource_id: String,
        resource_type: String,
    },

    /// Resource was deleted
    ResourceDeleted {
        resource_id: String,
        resource_type: String,
    },

    /// Resource attributes changed (affects ABAC evaluation)
    /// This is critical for attribute-based access control
    ResourceAttributesChanged {
        resource_id: String,
        resource_type: String,
    },

    // ===== Permission-related events =====
    /// New permission was created
    PermissionCreated { permission_id: String },

    /// Permission definition was updated
    PermissionUpdated { permission_id: String },

    /// Permission was deleted
    PermissionDeleted { permission_id: String },

    // ===== System-wide events =====
    /// System configuration changed
    /// Requires full cache invalidation
    ConfigurationChanged,

    /// System maintenance mode
    /// Requires full cache invalidation
    SystemMaintenance,

    /// Emergency cache clear
    /// Clears all caches immediately
    ClearAll,
}

impl InvalidationEvent {
    /// Get a human-readable description of the event
    pub fn description(&self) -> String {
        match self {
            Self::UserUpdated { user_id } => format!("User {} updated", user_id),
            Self::UserDeleted { user_id } => format!("User {} deleted", user_id),
            Self::UserPermissionsChanged { user_id } => {
                format!("Permissions changed for user {}", user_id)
            }
            Self::UserAttributesChanged { user_id } => {
                format!("Attributes changed for user {}", user_id)
            }

            Self::RoleCreated { role_id } => format!("Role {} created", role_id),
            Self::RoleUpdated { role_id } => format!("Role {} updated", role_id),
            Self::RoleDeleted { role_id } => format!("Role {} deleted", role_id),
            Self::RolePermissionsChanged { role_id } => {
                format!("Permissions changed for role {}", role_id)
            }

            Self::PolicyCreated { policy_id } => format!("Policy {} created", policy_id),
            Self::PolicyUpdated { policy_id } => format!("Policy {} updated", policy_id),
            Self::PolicyDeleted { policy_id } => format!("Policy {} deleted", policy_id),
            Self::PolicyActivated { policy_id } => format!("Policy {} activated", policy_id),
            Self::PolicyDeactivated { policy_id } => format!("Policy {} deactivated", policy_id),

            Self::ResourceCreated {
                resource_id,
                resource_type,
            } => format!("Resource {} of type {} created", resource_id, resource_type),
            Self::ResourceUpdated {
                resource_id,
                resource_type,
            } => format!("Resource {} of type {} updated", resource_id, resource_type),
            Self::ResourceDeleted {
                resource_id,
                resource_type,
            } => format!("Resource {} of type {} deleted", resource_id, resource_type),
            Self::ResourceAttributesChanged {
                resource_id,
                resource_type,
            } => format!(
                "Attributes changed for resource {} of type {}",
                resource_id, resource_type
            ),

            Self::PermissionCreated { permission_id } => {
                format!("Permission {} created", permission_id)
            }
            Self::PermissionUpdated { permission_id } => {
                format!("Permission {} updated", permission_id)
            }
            Self::PermissionDeleted { permission_id } => {
                format!("Permission {} deleted", permission_id)
            }

            Self::ConfigurationChanged => "System configuration changed".to_string(),
            Self::SystemMaintenance => "System maintenance mode".to_string(),
            Self::ClearAll => "Emergency cache clear".to_string(),
        }
    }

    /// Check if this event requires immediate processing
    /// Some events are critical and should bypass batching
    pub fn is_critical(&self) -> bool {
        matches!(
            self,
            Self::ConfigurationChanged
                | Self::SystemMaintenance
                | Self::ClearAll
                | Self::PolicyActivated { .. }
                | Self::PolicyDeactivated { .. }
        )
    }

    /// Get the scope of invalidation (user-specific, role-specific, or global)
    pub fn scope(&self) -> InvalidationScope {
        match self {
            Self::UserUpdated { .. }
            | Self::UserDeleted { .. }
            | Self::UserPermissionsChanged { .. }
            | Self::UserAttributesChanged { .. } => InvalidationScope::User,

            Self::RoleCreated { .. }
            | Self::RoleUpdated { .. }
            | Self::RoleDeleted { .. }
            | Self::RolePermissionsChanged { .. } => InvalidationScope::Role,

            Self::PolicyCreated { .. }
            | Self::PolicyUpdated { .. }
            | Self::PolicyDeleted { .. }
            | Self::PolicyActivated { .. }
            | Self::PolicyDeactivated { .. } => InvalidationScope::Policy,

            Self::ResourceCreated { .. }
            | Self::ResourceUpdated { .. }
            | Self::ResourceDeleted { .. }
            | Self::ResourceAttributesChanged { .. } => InvalidationScope::Resource,

            Self::PermissionCreated { .. }
            | Self::PermissionUpdated { .. }
            | Self::PermissionDeleted { .. } => InvalidationScope::Permission,

            Self::ConfigurationChanged | Self::SystemMaintenance | Self::ClearAll => {
                InvalidationScope::Global
            }
        }
    }
}

/// Scope of cache invalidation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvalidationScope {
    /// Affects a single user's cache
    User,
    /// Affects all users with a specific role
    Role,
    /// Affects evaluations matching a policy
    Policy,
    /// Affects evaluations for a specific resource
    Resource,
    /// Affects all permission-based evaluations
    Permission,
    /// Affects all caches globally
    Global,
}

impl InvalidationScope {
    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::User => "User-specific cache",
            Self::Role => "Role-specific cache",
            Self::Policy => "Policy-specific cache",
            Self::Resource => "Resource-specific cache",
            Self::Permission => "Permission-specific cache",
            Self::Global => "All caches",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_equality() {
        let event1 = InvalidationEvent::UserPermissionsChanged { user_id: 1 };
        let event2 = InvalidationEvent::UserPermissionsChanged { user_id: 1 };
        let event3 = InvalidationEvent::UserPermissionsChanged { user_id: 2 };

        assert_eq!(event1, event2);
        assert_ne!(event1, event3);
    }

    #[test]
    fn test_event_description() {
        let event = InvalidationEvent::UserPermissionsChanged { user_id: 42 };
        assert_eq!(event.description(), "Permissions changed for user 42");

        let event = InvalidationEvent::RolePermissionsChanged {
            role_id: "admin".to_string(),
        };
        assert_eq!(event.description(), "Permissions changed for role admin");

        let event = InvalidationEvent::PolicyUpdated {
            policy_id: "policy-1".to_string(),
        };
        assert_eq!(event.description(), "Policy policy-1 updated");
    }

    #[test]
    fn test_critical_events() {
        assert!(InvalidationEvent::ConfigurationChanged.is_critical());
        assert!(InvalidationEvent::SystemMaintenance.is_critical());
        assert!(InvalidationEvent::ClearAll.is_critical());
        assert!(InvalidationEvent::PolicyActivated {
            policy_id: "p1".to_string()
        }
        .is_critical());

        assert!(!InvalidationEvent::UserUpdated { user_id: 1 }.is_critical());
        assert!(!InvalidationEvent::PolicyUpdated {
            policy_id: "p1".to_string()
        }
        .is_critical());
    }

    #[test]
    fn test_event_scope() {
        assert_eq!(
            InvalidationEvent::UserPermissionsChanged { user_id: 1 }.scope(),
            InvalidationScope::User
        );

        assert_eq!(
            InvalidationEvent::RolePermissionsChanged {
                role_id: "admin".to_string()
            }
            .scope(),
            InvalidationScope::Role
        );

        assert_eq!(
            InvalidationEvent::PolicyUpdated {
                policy_id: "p1".to_string()
            }
            .scope(),
            InvalidationScope::Policy
        );

        assert_eq!(
            InvalidationEvent::ResourceAttributesChanged {
                resource_id: "r1".to_string(),
                resource_type: "document".to_string()
            }
            .scope(),
            InvalidationScope::Resource
        );

        assert_eq!(
            InvalidationEvent::ConfigurationChanged.scope(),
            InvalidationScope::Global
        );
    }

    #[test]
    fn test_scope_description() {
        assert_eq!(InvalidationScope::User.description(), "User-specific cache");
        assert_eq!(InvalidationScope::Global.description(), "All caches");
    }
}
