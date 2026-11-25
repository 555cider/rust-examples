pub mod abac;
pub mod cache;
pub mod hybrid;
pub mod manager;
pub mod rbac;
pub mod resilience;

// Re-export main types for convenient access
pub use abac::PolicyEngine;
pub use hybrid::{
    EvaluationDetails, HybridContext, HybridDecision, HybridEvaluator, HybridEvaluatorStats,
    HybridRequest, HybridResult,
};
pub use manager::{
    AccessContext, AccessControlDecision, AccessControlManager, AccessControlManagerStats,
    AccessControlRequest, AccessControlResponse, AppliedPolicy,
    PerformanceMetrics as AccessControlPerformanceMetrics, PolicyEffect, PolicyType,
    ResponseMetadata,
};
pub use rbac::RbacEngine;
pub use resilience::{
    run_circuit_breaker_demonstration, AccessControlErrorHandler, CircuitBreakerConfig,
    DegradationStrategy, ErrorHandlingStatistics, HealthCheckSummary, SystemHealthStatus,
};
