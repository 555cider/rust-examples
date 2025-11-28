use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::env::var;
use std::sync::Arc;

/// Access control mode configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AccessControlMode {
    /// RBAC only mode - traditional role-based access control
    #[default]
    RbacOnly,
    /// Hybrid mode - combines RBAC and ABAC
    Hybrid,
}

impl std::str::FromStr for AccessControlMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rbac" | "rbac_only" => Ok(AccessControlMode::RbacOnly),
            "hybrid" => Ok(AccessControlMode::Hybrid),
            _ => Err(format!("Invalid access control mode: {}", s)),
        }
    }
}

impl std::fmt::Display for AccessControlMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccessControlMode::RbacOnly => write!(f, "rbac_only"),
            AccessControlMode::Hybrid => write!(f, "hybrid"),
        }
    }
}

/// Combination strategy for hybrid mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CombinationStrategy {
    /// RBAC first, fallback to ABAC if RBAC denies
    RbacFirst,
    /// ABAC first, fallback to RBAC if ABAC denies
    AbacFirst,
    /// Both RBAC and ABAC must permit
    BothRequired,
    /// Either RBAC or ABAC can permit
    #[default]
    EitherAllowed,
}

impl std::str::FromStr for CombinationStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rbac_first" => Ok(CombinationStrategy::RbacFirst),
            "abac_first" => Ok(CombinationStrategy::AbacFirst),
            "both_required" => Ok(CombinationStrategy::BothRequired),
            "either_allowed" => Ok(CombinationStrategy::EitherAllowed),
            _ => Err(format!("Invalid combination strategy: {}", s)),
        }
    }
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConflictResolution {
    /// Deny takes precedence over permit
    #[default]
    DenyOverrides,
    /// Permit takes precedence over deny
    PermitOverrides,
    /// First applicable result wins
    FirstApplicable,
}

impl std::str::FromStr for ConflictResolution {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "deny_overrides" => Ok(ConflictResolution::DenyOverrides),
            "permit_overrides" => Ok(ConflictResolution::PermitOverrides),
            "first_applicable" => Ok(ConflictResolution::FirstApplicable),
            _ => Err(format!("Invalid conflict resolution: {}", s)),
        }
    }
}

/// Access control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlConfig {
    /// Current access control mode
    pub mode: AccessControlMode,
    /// Combination strategy for hybrid mode
    pub combination_strategy: CombinationStrategy,
    /// Conflict resolution strategy
    pub conflict_resolution: ConflictResolution,
    /// Enable caching for access control decisions
    pub enable_caching: bool,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Maximum evaluation time in milliseconds
    pub max_evaluation_time_ms: u64,
    /// Enable audit logging
    pub enable_audit_logging: bool,
}

impl AccessControlConfig {
    pub fn from_env() -> Self {
        let mode = var("ACCESS_CONTROL_MODE")
            .unwrap_or_else(|_| "rbac_only".to_string())
            .parse()
            .unwrap_or_default();

        let combination_strategy = var("ACCESS_CONTROL_COMBINATION_STRATEGY")
            .unwrap_or_else(|_| "either_allowed".to_string())
            .parse()
            .unwrap_or_default();

        let conflict_resolution = var("ACCESS_CONTROL_CONFLICT_RESOLUTION")
            .unwrap_or_else(|_| "deny_overrides".to_string())
            .parse()
            .unwrap_or_default();

        let enable_caching = var("ACCESS_CONTROL_ENABLE_CACHING")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        let cache_ttl_seconds = var("ACCESS_CONTROL_CACHE_TTL_SECONDS")
            .unwrap_or_else(|_| "300".to_string())
            .parse()
            .unwrap_or(300);

        let max_evaluation_time_ms = var("ACCESS_CONTROL_MAX_EVALUATION_TIME_MS")
            .unwrap_or_else(|_| "100".to_string())
            .parse()
            .unwrap_or(100);

        let enable_audit_logging = var("ACCESS_CONTROL_ENABLE_AUDIT_LOGGING")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        Self {
            mode,
            combination_strategy,
            conflict_resolution,
            enable_caching,
            cache_ttl_seconds,
            max_evaluation_time_ms,
            enable_audit_logging,
        }
    }

    /// Check if ABAC is enabled
    pub fn is_abac_enabled(&self) -> bool {
        matches!(self.mode, AccessControlMode::Hybrid)
    }

    /// Check if RBAC is enabled
    pub fn is_rbac_enabled(&self) -> bool {
        true // RBAC is always enabled
    }
}

/// Runtime configuration manager for access control
/// Uses ArcSwap for lock-free reads and atomic updates
pub struct AccessControlConfigManager {
    config: Arc<ArcSwap<AccessControlConfig>>,
}

impl AccessControlConfigManager {
    pub fn new(config: AccessControlConfig) -> Self {
        Self {
            config: Arc::new(ArcSwap::from_pointee(config)),
        }
    }

    /// Get current configuration (lock-free read)
    /// Returns an Arc to the current config, avoiding clones
    pub fn get_config(&self) -> Arc<AccessControlConfig> {
        self.config.load_full()
    }

    /// Get a cloned copy of the current configuration
    pub fn get_config_clone(&self) -> AccessControlConfig {
        (**self.config.load()).clone()
    }

    /// Update access control mode at runtime (atomic swap)
    pub fn set_mode(&self, mode: AccessControlMode) -> Result<(), String> {
        self.config.rcu(|current| {
            let mut new_config = (**current).clone();
            new_config.mode = mode;
            new_config
        });
        tracing::info!("Access control mode changed to: {}", mode);
        Ok(())
    }

    /// Update combination strategy at runtime (atomic swap)
    pub fn set_combination_strategy(&self, strategy: CombinationStrategy) -> Result<(), String> {
        self.config.rcu(|current| {
            let mut new_config = (**current).clone();
            new_config.combination_strategy = strategy;
            new_config
        });
        tracing::info!("Combination strategy changed to: {:?}", strategy);
        Ok(())
    }

    /// Update conflict resolution at runtime (atomic swap)
    pub fn set_conflict_resolution(&self, resolution: ConflictResolution) -> Result<(), String> {
        self.config.rcu(|current| {
            let mut new_config = (**current).clone();
            new_config.conflict_resolution = resolution;
            new_config
        });
        tracing::info!("Conflict resolution changed to: {:?}", resolution);
        Ok(())
    }

    /// Enable or disable caching at runtime (atomic swap)
    pub fn set_caching_enabled(&self, enabled: bool) -> Result<(), String> {
        self.config.rcu(|current| {
            let mut new_config = (**current).clone();
            new_config.enable_caching = enabled;
            new_config
        });
        tracing::info!(
            "Access control caching {}",
            if enabled { "enabled" } else { "disabled" }
        );
        Ok(())
    }

    /// Update cache TTL at runtime (atomic swap)
    pub fn set_cache_ttl(&self, ttl_seconds: u64) -> Result<(), String> {
        self.config.rcu(|current| {
            let mut new_config = (**current).clone();
            new_config.cache_ttl_seconds = ttl_seconds;
            new_config
        });
        tracing::info!("Cache TTL changed to: {} seconds", ttl_seconds);
        Ok(())
    }

    /// Update maximum evaluation time at runtime (atomic swap)
    pub fn set_max_evaluation_time(&self, max_time_ms: u64) -> Result<(), String> {
        self.config.rcu(|current| {
            let mut new_config = (**current).clone();
            new_config.max_evaluation_time_ms = max_time_ms;
            new_config
        });
        tracing::info!("Maximum evaluation time changed to: {} ms", max_time_ms);
        Ok(())
    }

    /// Enable or disable audit logging at runtime (atomic swap)
    pub fn set_audit_logging_enabled(&self, enabled: bool) -> Result<(), String> {
        self.config.rcu(|current| {
            let mut new_config = (**current).clone();
            new_config.enable_audit_logging = enabled;
            new_config
        });
        tracing::info!(
            "Audit logging {}",
            if enabled { "enabled" } else { "disabled" }
        );
        Ok(())
    }

    /// Update entire configuration atomically
    pub fn update_config(&self, new_config: AccessControlConfig) {
        self.config.store(Arc::new(new_config));
        tracing::info!("Access control configuration updated");
    }

    /// Get a clone of the config manager for sharing across threads
    pub fn clone_manager(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
        }
    }
}

impl Default for AccessControlConfigManager {
    fn default() -> Self {
        Self::new(AccessControlConfig::from_env())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_control_mode_parsing() {
        assert_eq!(
            "rbac".parse::<AccessControlMode>().unwrap(),
            AccessControlMode::RbacOnly
        );
        assert_eq!(
            "rbac_only".parse::<AccessControlMode>().unwrap(),
            AccessControlMode::RbacOnly
        );
        assert_eq!(
            "hybrid".parse::<AccessControlMode>().unwrap(),
            AccessControlMode::Hybrid
        );
        assert!("invalid".parse::<AccessControlMode>().is_err());
    }

    #[test]
    fn test_combination_strategy_parsing() {
        assert_eq!(
            "rbac_first".parse::<CombinationStrategy>().unwrap(),
            CombinationStrategy::RbacFirst
        );
        assert_eq!(
            "abac_first".parse::<CombinationStrategy>().unwrap(),
            CombinationStrategy::AbacFirst
        );
        assert_eq!(
            "both_required".parse::<CombinationStrategy>().unwrap(),
            CombinationStrategy::BothRequired
        );
        assert_eq!(
            "either_allowed".parse::<CombinationStrategy>().unwrap(),
            CombinationStrategy::EitherAllowed
        );
        assert!("invalid".parse::<CombinationStrategy>().is_err());
    }

    #[test]
    fn test_conflict_resolution_parsing() {
        assert_eq!(
            "deny_overrides".parse::<ConflictResolution>().unwrap(),
            ConflictResolution::DenyOverrides
        );
        assert_eq!(
            "permit_overrides".parse::<ConflictResolution>().unwrap(),
            ConflictResolution::PermitOverrides
        );
        assert_eq!(
            "first_applicable".parse::<ConflictResolution>().unwrap(),
            ConflictResolution::FirstApplicable
        );
        assert!("invalid".parse::<ConflictResolution>().is_err());
    }

    #[test]
    fn test_config_manager_runtime_updates() {
        let manager = AccessControlConfigManager::default();

        // Test mode change
        assert!(manager.set_mode(AccessControlMode::Hybrid).is_ok());
        assert_eq!(manager.get_config_clone().mode, AccessControlMode::Hybrid);

        // Test strategy change
        assert!(manager
            .set_combination_strategy(CombinationStrategy::BothRequired)
            .is_ok());
        assert_eq!(
            manager.get_config_clone().combination_strategy,
            CombinationStrategy::BothRequired
        );

        // Test caching toggle
        assert!(manager.set_caching_enabled(false).is_ok());
        assert!(!manager.get_config_clone().enable_caching);
    }
}
