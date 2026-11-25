use crate::{
    auth::AuthService,
    config::access_control::{AccessControlConfig, AccessControlConfigManager},
    repositories::{
        AccessEvaluationLogRepository, AuthRepository, HistoryRepository, PermissionRepository,
        PolicyRepository, UserRepository, RoleRepository,
    },
    services::{
        access_control::{AccessControlManager, HybridEvaluator, PolicyEngine, RbacEngine},
        access_evaluation_log::AccessEvaluationLogService,
        history::HistoryService,
        permission::PermissionService,
        user::UserService,
        role::RoleService,
    },
};
use std::sync::Arc;

#[derive(Clone)]
pub struct ServiceContainer {
    pub access_evaluation_log_service: Arc<AccessEvaluationLogService>,
    pub auth_service: Arc<AuthService>,
    pub history_service: Arc<HistoryService>,
    pub permission_service: Arc<PermissionService>,
    pub user_service: Arc<UserService>,
    pub role_service: Arc<RoleService>,
    pub policy_repository: Arc<PolicyRepository>,
    pub access_control_manager: Option<Arc<AccessControlManager>>,
}

impl ServiceContainer {
    pub fn new(db: Arc<sqlx::SqlitePool>) -> Self {
        let access_evaluation_log_repo = AccessEvaluationLogRepository::new(db.clone());
        let auth_repo = AuthRepository::new(db.clone());
        let history_repo = HistoryRepository::new(db.clone());
        let permission_repo = PermissionRepository::new(db.clone());
        let policy_repo = PolicyRepository::new(db.clone());
        let user_repo = UserRepository::new(db.clone());
        let role_repo = RoleRepository::new(db.clone());

        let access_evaluation_log =
            Arc::new(AccessEvaluationLogService::new(access_evaluation_log_repo));
        let history = Arc::new(HistoryService::new(history_repo));
        let auth = Arc::new(AuthService::new(
            auth_repo,
            user_repo.clone(),
            role_repo.clone(),
            permission_repo.clone(),
            history.clone(),
        ));
        let permission = Arc::new(PermissionService::new(permission_repo, user_repo.clone()));
        let user = Arc::new(UserService::new(user_repo));
        let role_service = Arc::new(RoleService::new(role_repo));

        // Initialize access control manager if hybrid access control is enabled
        let access_control_manager =
            Self::create_access_control_manager(db.clone(), access_evaluation_log.clone());

        Self {
            access_evaluation_log_service: access_evaluation_log,
            auth_service: auth,
            history_service: history,
            permission_service: permission,
            user_service: user,
            role_service,
            policy_repository: Arc::new(policy_repo),
            access_control_manager,
        }
    }

    fn create_access_control_manager(
        db: Arc<sqlx::SqlitePool>,
        audit_service: Arc<AccessEvaluationLogService>,
    ) -> Option<Arc<AccessControlManager>> {
        // Try to create access control manager, but don't fail if dependencies are missing
        match Self::try_create_access_control_manager(db, audit_service) {
            Ok(manager) => Some(Arc::new(manager)),
            Err(e) => {
                tracing::warn!("Failed to initialize access control manager: {}", e);
                None
            }
        }
    }

    fn try_create_access_control_manager(
        db: Arc<sqlx::SqlitePool>,
        audit_service: Arc<AccessEvaluationLogService>,
    ) -> Result<AccessControlManager, Box<dyn std::error::Error + Send + Sync>> {
        // Create repositories
        let permission_repo = PermissionRepository::new(db.clone());
        let user_repo = UserRepository::new(db.clone());
        let role_repo = RoleRepository::new(db.clone());
        let policy_repo = PolicyRepository::new(db.clone());

        // Load access control configuration from environment
        let config = AccessControlConfig::from_env();

        // Create RBAC engine
        let rbac_engine =
            RbacEngine::new(permission_repo, user_repo, role_repo, config.clone());

        // Create ABAC policy engine
        let policy_engine = PolicyEngine::new(policy_repo);

        // Create hybrid evaluator
        let hybrid_evaluator =
            HybridEvaluator::new(rbac_engine.clone(), policy_engine.clone(), config.clone());

        // Create configuration manager
        let config_manager = AccessControlConfigManager::new(config);

        // Create and return access control manager
        Ok(AccessControlManager::new(
            rbac_engine,
            policy_engine,
            hybrid_evaluator,
            audit_service.as_ref().clone(),
            config_manager,
        ))
    }
}
