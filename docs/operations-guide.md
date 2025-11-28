# Hybrid Access Control Operations Guide

## Overview

This guide provides operational procedures for managing and maintaining the Hybrid Access Control System in production environments. It covers day-to-day operations, monitoring, troubleshooting, and best practices.

## Table of Contents

1. [System Architecture Overview](#system-architecture-overview)
2. [Configuration Management](#configuration-management)
3. [Policy Management](#policy-management)
4. [Monitoring and Alerting](#monitoring-and-alerting)
5. [Performance Tuning](#performance-tuning)
6. [Security Operations](#security-operations)
7. [Backup and Recovery](#backup-and-recovery)
8. [Troubleshooting](#troubleshooting)
9. [Maintenance Procedures](#maintenance-procedures)
10. [Best Practices](#best-practices)

## System Architecture Overview

### Components

The Hybrid Access Control System consists of several key components:

- **Access Control Manager**: Central coordinator for all access decisions
- **RBAC Engine**: Handles role-based access control
- **ABAC Engine**: Processes attribute-based policies
- **Hybrid Evaluator**: Combines RBAC and ABAC results
- **Policy Engine**: Evaluates ABAC policies
- **Attribute Service**: Manages user, resource, and environment attributes
- **Cache Manager**: Provides multi-level caching for performance
- **Audit Logger**: Records all access evaluations

### Data Flow

```
Request → Access Control Manager → [RBAC Engine + ABAC Engine] → Hybrid Evaluator → Response
                                                ↓
                                        Audit Logger → Database
```

## Configuration Management

### Environment Configuration

Key environment variables for production:

```env
# Core Settings
ACCESS_CONTROL_MODE=hybrid
ACCESS_CONTROL_COMBINATION_STRATEGY=EitherAllowed
ACCESS_CONTROL_CONFLICT_RESOLUTION=DenyOverrides

# Performance Settings
ACCESS_CONTROL_ENABLE_CACHING=true
ACCESS_CONTROL_CACHE_TTL_SECONDS=600
ACCESS_CONTROL_CACHE_SIZE=10000
ACCESS_CONTROL_MAX_EVALUATION_TIME_MS=100

# Reliability Settings
ACCESS_CONTROL_CIRCUIT_BREAKER_FAILURE_THRESHOLD=5
ACCESS_CONTROL_CIRCUIT_BREAKER_TIMEOUT_SECONDS=60
ACCESS_CONTROL_ENABLE_GRACEFUL_DEGRADATION=true

# Audit Settings
ACCESS_CONTROL_ENABLE_AUDIT_LOGGING=true
ACCESS_CONTROL_LOG_LEVEL=INFO
ACCESS_CONTROL_AUDIT_RETENTION_DAYS=90

# Security Configuration (NEW)
# Super admin role identifiers (comma-separated)
SUPER_ADMIN_ROLES=super_admin,system_admin

# Super admin user type IDs (comma-separated)
SUPER_ADMIN_USER_TYPE_IDS=1

# Require MFA for admin operations
REQUIRE_MFA_FOR_ADMIN=true

# Allowed permission code patterns (regex, comma-separated)
ALLOWED_PERMISSION_PATTERNS=^[a-z_]+\.[a-z_]+$

# Sensitive fields to redact in logs (comma-separated)
LOG_REDACTION_FIELDS=password,token,secret,api_key,auth_token,ssn,credit_card

# Metrics and Monitoring (NEW)
# Enable Prometheus metrics endpoint
ACCESS_CONTROL_ENABLE_METRICS=true

# Metrics collection interval in seconds
ACCESS_CONTROL_METRICS_INTERVAL_SECONDS=60

# Maximum metrics history to keep in memory
ACCESS_CONTROL_METRICS_MAX_HISTORY=10000

# Performance warning thresholds
ACCESS_CONTROL_WARN_EVALUATION_TIME_MS=100
ACCESS_CONTROL_WARN_CACHE_HIT_RATE=0.7
ACCESS_CONTROL_WARN_ERROR_RATE=0.01

# Retry Configuration (NEW)
# Maximum number of retry attempts for transient failures
ACCESS_CONTROL_RETRY_ATTEMPTS=3

# Initial retry backoff in milliseconds
ACCESS_CONTROL_RETRY_BACKOFF_MS=100

# Maximum retry backoff in milliseconds
ACCESS_CONTROL_RETRY_MAX_BACKOFF_MS=5000

# Retry backoff multiplier
ACCESS_CONTROL_RETRY_BACKOFF_MULTIPLIER=2.0
```

### Environment Variable Descriptions

#### Security Configuration

- **SUPER_ADMIN_ROLES**: Comma-separated list of role names that have super admin privileges. These roles bypass normal permission checks.
  - Default: `super_admin,system_admin`
  - Example: `super_admin,root,system_admin`

- **SUPER_ADMIN_USER_TYPE_IDS**: Comma-separated list of user type IDs that have super admin privileges.
  - Default: `1`
  - Example: `1,2`
  - Note: Replaces hardcoded `user_type_id == 1` checks

- **REQUIRE_MFA_FOR_ADMIN**: Whether to require multi-factor authentication for administrative operations.
  - Default: `true`
  - Values: `true`, `false`

- **ALLOWED_PERMISSION_PATTERNS**: Regex patterns for validating permission codes. Prevents injection of malformed permission codes.
  - Default: `^[a-z_]+\.[a-z_]+$`
  - Example: `^[a-z_]+\.[a-z_]+$,^admin\.[a-z_]+\.[a-z_]+$`

- **LOG_REDACTION_FIELDS**: Fields to automatically redact in logs to protect sensitive information.
  - Default: `password,token,secret`
  - Example: `password,token,secret,api_key,auth_token,ssn,credit_card`

#### Metrics and Monitoring

- **ACCESS_CONTROL_ENABLE_METRICS**: Enable Prometheus metrics endpoint at `/api/v1/access-control/metrics`.
  - Default: `true`
  - Values: `true`, `false`

- **ACCESS_CONTROL_METRICS_INTERVAL_SECONDS**: How often to aggregate metrics.
  - Default: `60`
  - Range: `10-300` seconds

- **ACCESS_CONTROL_METRICS_MAX_HISTORY**: Maximum number of metric data points to keep in memory.
  - Default: `10000`
  - Range: `1000-100000`

- **ACCESS_CONTROL_WARN_EVALUATION_TIME_MS**: Log warning if evaluation time exceeds this threshold.
  - Default: `100`
  - Range: `50-1000` milliseconds

- **ACCESS_CONTROL_WARN_CACHE_HIT_RATE**: Log warning if cache hit rate falls below this threshold.
  - Default: `0.7` (70%)
  - Range: `0.0-1.0`

- **ACCESS_CONTROL_WARN_ERROR_RATE**: Log warning if error rate exceeds this threshold.
  - Default: `0.01` (1%)
  - Range: `0.0-1.0`

#### Retry Configuration

- **ACCESS_CONTROL_RETRY_ATTEMPTS**: Maximum number of retry attempts for transient failures (database connection, timeout).
  - Default: `3`
  - Range: `0-10`

- **ACCESS_CONTROL_RETRY_BACKOFF_MS**: Initial backoff time before first retry.
  - Default: `100`
  - Range: `10-1000` milliseconds

- **ACCESS_CONTROL_RETRY_MAX_BACKOFF_MS**: Maximum backoff time between retries.
  - Default: `5000`
  - Range: `1000-30000` milliseconds

- **ACCESS_CONTROL_RETRY_BACKOFF_MULTIPLIER**: Multiplier for exponential backoff.
  - Default: `2.0`
  - Range: `1.0-5.0`

### Runtime Configuration Changes

Most configuration changes can be applied without restart:

```bash
# Change access control mode
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"mode": "hybrid"}' \
     http://localhost:3000/api/v1/access-control/config/mode

# Update cache settings
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"enabled": true, "ttl_seconds": 900}' \
     http://localhost:3000/api/v1/access-control/config/caching

# Adjust evaluation timeout
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"max_time_ms": 150}' \
     http://localhost:3000/api/v1/access-control/config/evaluation-timeout
```

### Configuration Validation

Validate configuration changes before applying:

```bash
# Check current configuration
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
     http://localhost:3000/api/v1/access-control/config

# Validate system health after changes
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
     http://localhost:3000/api/v1/access-control/health
```

## Policy Management

### Policy Lifecycle

1. **Development**: Create and test policies in development environment
2. **Validation**: Validate policy syntax and logic
3. **Staging**: Deploy to staging for integration testing
4. **Production**: Deploy to production with monitoring
5. **Monitoring**: Track policy effectiveness and performance
6. **Updates**: Modify policies based on requirements
7. **Retirement**: Deactivate and archive obsolete policies

### Creating Policies

#### Basic Policy Structure

```json
{
  "id": "unique-policy-id",
  "name": "Human Readable Policy Name",
  "description": "Detailed description of what this policy does",
  "target": {
    "subjects": ["user.department == 'Engineering'"],
    "resources": ["resource.type == 'code_repository'"],
    "actions": ["read", "write"]
  },
  "rules": [
    {
      "condition": {
        "and": [
          {
            "attribute": "user.clearance_level",
            "operator": ">=",
            "value": 2
          },
          {
            "attribute": "environment.network_zone",
            "operator": "==",
            "value": "internal"
          }
        ]
      },
      "effect": "PERMIT"
    }
  ],
  "priority": 100,
  "is_active": true
}
```

#### Policy Validation

Always validate policies before deployment:

```bash
# Validate policy syntax
curl -X POST \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d @new-policy.json \
     http://localhost:3000/api/v1/policies/validate

# Test policy with sample data
curl -X POST \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "policy_id": "new-policy-id",
       "test_context": {
         "user": {"department": "Engineering", "clearance_level": 3},
         "resource": {"type": "code_repository", "id": "repo-123"},
         "action": "read",
         "environment": {"network_zone": "internal"}
       }
     }' \
     http://localhost:3000/api/v1/policies/test
```

### Policy Deployment

#### Gradual Rollout

Deploy policies gradually to minimize risk:

```bash
# 1. Deploy policy in inactive state
curl -X POST \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d @policy-inactive.json \
     http://localhost:3000/api/v1/policies

# 2. Test with specific users/resources
curl -X POST \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{
       "policy_id": "new-policy",
       "test_mode": true,
       "test_subjects": ["user123", "user456"]
     }' \
     http://localhost:3000/api/v1/policies/test-deployment

# 3. Activate policy for production
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"is_active": true}' \
     http://localhost:3000/api/v1/policies/new-policy
```

#### Policy Updates

Update existing policies safely:

```bash
# 1. Create new version of policy
curl -X POST \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d @policy-v2.json \
     http://localhost:3000/api/v1/policies

# 2. Test new version
curl -X POST \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"policy_id": "policy-v2", "test_context": {...}}' \
     http://localhost:3000/api/v1/policies/test

# 3. Switch traffic to new version
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"is_active": false}' \
     http://localhost:3000/api/v1/policies/policy-v1

curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"is_active": true}' \
     http://localhost:3000/api/v1/policies/policy-v2
```

## Monitoring and Alerting

### Key Metrics

Monitor these critical metrics:

#### System Health Metrics
- Overall system health status
- Component health (RBAC Engine, ABAC Engine)
- Circuit breaker states
- Degradation status

#### Performance Metrics
- Average evaluation time
- 95th percentile evaluation time
- Cache hit rate
- Throughput (evaluations per second)

#### Business Metrics
- Access grant/deny ratios
- Policy effectiveness
- User access patterns
- Resource access frequency

### Health Monitoring

```bash
# Check overall system health
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     http://localhost:3000/api/v1/access-control/health

# Get detailed statistics
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     http://localhost:3000/api/v1/access-control/health/statistics

# Monitor specific components
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     http://localhost:3000/api/v1/access-control/health | \
     jq '.components[] | select(.name == "ABAC Engine")'
```

### Performance Monitoring

```bash
# Monitor evaluation performance
watch -n 10 'curl -s -H "Authorization: Bearer $MONITOR_TOKEN" \
  http://localhost:3000/api/v1/access-control/health/statistics | \
  jq ".evaluation_stats | {avg_time: .average_evaluation_time_ms, cache_hit_rate: .cache_hit_rate}"'

# Check cache performance
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     http://localhost:3000/api/v1/access-control/health/statistics | \
     jq '.evaluation_stats.cache_hit_rate'

# Monitor throughput
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     "http://localhost:3000/api/v1/access-control/logs/statistics?from_date=$(date -d '1 hour ago' -Iseconds)" | \
     jq '.total_evaluations'
```

### Alerting Rules

Set up alerts for critical conditions:

#### Critical Alerts (Immediate Response)
- System health: overall_health == false
- High error rate: error_rate > 5%
- Evaluation timeout: avg_evaluation_time > 200ms
- Circuit breaker open: circuit_breaker_state == "open"
- Database connection failure: database_errors > 10 per minute

#### Warning Alerts (Monitor Closely)
- Cache hit rate: cache_hit_rate < 70%
- High evaluation time: avg_evaluation_time > 100ms
- Degraded mode: is_degraded == true
- High deny rate: deny_rate > 20%
- Memory usage: cache_memory_mb > 500

#### Prometheus Alert Configuration

```yaml
# access_control_alerts.yml
groups:
  - name: access_control_critical
    interval: 30s
    rules:
      - alert: AccessControlSystemUnhealthy
        expr: access_control_system_health == 0
        for: 2m
        labels:
          severity: critical
          component: access_control
        annotations:
          summary: "Access control system is unhealthy"
          description: "Overall system health check failed"
          runbook_url: "https://docs.example.com/runbooks/access-control-unhealthy"
      
      - alert: HighAccessControlErrorRate
        expr: rate(access_control_error_count[5m]) > 0.05
        for: 5m
        labels:
          severity: critical
          component: access_control
        annotations:
          summary: "High error rate in access control system"
          description: "Error rate is {{ $value | humanizePercentage }}"
          runbook_url: "https://docs.example.com/runbooks/high-error-rate"
      
      - alert: AccessControlEvaluationTimeout
        expr: histogram_quantile(0.95, rate(access_control_evaluation_duration_ms_bucket[5m])) > 200
        for: 10m
        labels:
          severity: critical
          component: access_control
        annotations:
          summary: "Access control evaluations timing out"
          description: "95th percentile evaluation time is {{ $value }}ms"
          runbook_url: "https://docs.example.com/runbooks/evaluation-timeout"
      
      - alert: CircuitBreakerOpen
        expr: access_control_circuit_breaker_state{state="open"} == 1
        for: 1m
        labels:
          severity: critical
          component: access_control
        annotations:
          summary: "Circuit breaker is open for {{ $labels.component }}"
          description: "Component {{ $labels.component }} circuit breaker is open"
          runbook_url: "https://docs.example.com/runbooks/circuit-breaker-open"
      
      - alert: DatabaseConnectionFailure
        expr: rate(access_control_error_count{type="database"}[1m]) > 10
        for: 2m
        labels:
          severity: critical
          component: access_control
        annotations:
          summary: "High rate of database connection failures"
          description: "Database errors: {{ $value }} per minute"
          runbook_url: "https://docs.example.com/runbooks/database-failure"

  - name: access_control_warning
    interval: 60s
    rules:
      - alert: LowCacheHitRate
        expr: access_control_cache_hit_rate < 0.7
        for: 15m
        labels:
          severity: warning
          component: access_control
        annotations:
          summary: "Low cache hit rate"
          description: "Cache hit rate is {{ $value | humanizePercentage }}"
          runbook_url: "https://docs.example.com/runbooks/low-cache-hit-rate"
      
      - alert: HighEvaluationLatency
        expr: histogram_quantile(0.95, rate(access_control_evaluation_duration_ms_bucket[5m])) > 100
        for: 10m
        labels:
          severity: warning
          component: access_control
        annotations:
          summary: "High access control evaluation latency"
          description: "95th percentile evaluation time is {{ $value }}ms"
          runbook_url: "https://docs.example.com/runbooks/high-latency"
      
      - alert: AccessControlDegraded
        expr: access_control_degraded_mode == 1
        for: 5m
        labels:
          severity: warning
          component: access_control
        annotations:
          summary: "Access control system in degraded mode"
          description: "System is operating in degraded mode"
          runbook_url: "https://docs.example.com/runbooks/degraded-mode"
      
      - alert: HighDenyRate
        expr: rate(access_control_evaluation_total{result="deny"}[10m]) / rate(access_control_evaluation_total[10m]) > 0.2
        for: 15m
        labels:
          severity: warning
          component: access_control
        annotations:
          summary: "High access denial rate"
          description: "Deny rate is {{ $value | humanizePercentage }}"
          runbook_url: "https://docs.example.com/runbooks/high-deny-rate"
      
      - alert: HighCacheMemoryUsage
        expr: access_control_cache_memory_bytes / 1024 / 1024 > 500
        for: 30m
        labels:
          severity: warning
          component: access_control
        annotations:
          summary: "High cache memory usage"
          description: "Cache is using {{ $value }}MB of memory"
          runbook_url: "https://docs.example.com/runbooks/high-memory-usage"
```

#### Grafana Dashboard Configuration

```json
{
  "dashboard": {
    "title": "Access Control System",
    "panels": [
      {
        "title": "Evaluation Latency (95th percentile)",
        "targets": [
          {
            "expr": "histogram_quantile(0.95, rate(access_control_evaluation_duration_ms_bucket[5m]))"
          }
        ],
        "alert": {
          "conditions": [
            {
              "evaluator": {
                "params": [100],
                "type": "gt"
              }
            }
          ]
        }
      },
      {
        "title": "Cache Hit Rate",
        "targets": [
          {
            "expr": "access_control_cache_hit_rate"
          }
        ],
        "alert": {
          "conditions": [
            {
              "evaluator": {
                "params": [0.7],
                "type": "lt"
              }
            }
          ]
        }
      },
      {
        "title": "Error Rate",
        "targets": [
          {
            "expr": "rate(access_control_error_count[5m])"
          }
        ]
      },
      {
        "title": "Throughput",
        "targets": [
          {
            "expr": "rate(access_control_evaluation_total[1m])"
          }
        ]
      }
    ]
  }
}
```

### Log Analysis

```bash
# Monitor error patterns
tail -f logs/access_control.log | grep ERROR

# Analyze access patterns
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     "http://localhost:3000/api/v1/access-control/logs/statistics?group_by=resource_type" | \
     jq '.top_denied_resources'

# Check policy effectiveness
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     "http://localhost:3000/api/v1/access-control/logs?from_date=$(date -d '24 hours ago' -Iseconds)" | \
     jq '.logs | group_by(.applied_policies[]) | map({policy: .[0].applied_policies[0], count: length})'
```

## Performance Tuning

### Cache Optimization

#### Cache Configuration

```bash
# Optimal cache settings for different environments

# High-traffic production
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"enabled": true, "ttl_seconds": 900}' \
     http://localhost:3000/api/v1/access-control/config/caching

# Low-latency requirements
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"enabled": true, "ttl_seconds": 300}' \
     http://localhost:3000/api/v1/access-control/config/caching

# High-security environment (shorter TTL)
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"enabled": true, "ttl_seconds": 60}' \
     http://localhost:3000/api/v1/access-control/config/caching
```

#### Cache Monitoring

```bash
# Monitor cache effectiveness
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     http://localhost:3000/api/v1/access-control/health/statistics | \
     jq '.evaluation_stats | {
       cache_hit_rate: .cache_hit_rate,
       total_evaluations: .total_evaluations,
       cached_evaluations: (.total_evaluations * .cache_hit_rate)
     }'

# Cache invalidation patterns
grep "cache_invalidation" logs/access_control.log | \
  awk '{print $1, $2}' | sort | uniq -c
```

### Database Optimization

#### Index Management

```sql
-- Ensure proper indexes exist
CREATE INDEX IF NOT EXISTS idx_access_evaluation_log_user_id ON access_evaluation_log(user_id);
CREATE INDEX IF NOT EXISTS idx_access_evaluation_log_resource_type ON access_evaluation_log(resource_type);
CREATE INDEX IF NOT EXISTS idx_access_evaluation_log_created_at ON access_evaluation_log(created_at);
CREATE INDEX IF NOT EXISTS idx_user_attribute_user_id ON user_attribute(user_id);
CREATE INDEX IF NOT EXISTS idx_resource_attribute_resource_id ON resource_attribute(resource_id, resource_type);
CREATE INDEX IF NOT EXISTS idx_abac_policy_active ON abac_policy(is_active) WHERE is_active = true;
```

#### Query Optimization

```bash
# Monitor slow queries
sqlite3 data/sqlite.db "PRAGMA query_only = ON; EXPLAIN QUERY PLAN SELECT * FROM access_evaluation_log WHERE user_id = 123;"

# Analyze database performance
sqlite3 data/sqlite.db "PRAGMA optimize;"
```

### Application Tuning

#### Evaluation Timeout Optimization

```bash
# Start with conservative timeout
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"max_time_ms": 200}' \
     http://localhost:3000/api/v1/access-control/config/evaluation-timeout

# Monitor and adjust based on performance
# If 95th percentile < 50ms, reduce timeout
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"max_time_ms": 100}' \
     http://localhost:3000/api/v1/access-control/config/evaluation-timeout

# If timeouts occur frequently, increase
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"max_time_ms": 300}' \
     http://localhost:3000/api/v1/access-control/config/evaluation-timeout
```

#### Circuit Breaker Tuning

```env
# Conservative settings (fail-safe)
ACCESS_CONTROL_CIRCUIT_BREAKER_FAILURE_THRESHOLD=3
ACCESS_CONTROL_CIRCUIT_BREAKER_TIMEOUT_SECONDS=120

# Aggressive settings (performance-focused)
ACCESS_CONTROL_CIRCUIT_BREAKER_FAILURE_THRESHOLD=10
ACCESS_CONTROL_CIRCUIT_BREAKER_TIMEOUT_SECONDS=30
```

## Security Operations

### Access Control for the Access Control System

Secure the access control system itself:

```bash
# Restrict policy management to specific roles
curl -X POST \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{
       "id": "policy-management-access",
       "name": "Policy Management Access Control",
       "target": {
         "subjects": ["user.role == \"security_admin\""],
         "resources": ["resource.type == \"policy_management\""],
         "actions": ["create", "update", "delete"]
       },
       "rules": [{
         "condition": {
           "and": [
             {"attribute": "user.mfa_verified", "operator": "==", "value": true},
             {"attribute": "environment.network_zone", "operator": "==", "value": "internal"}
           ]
         },
         "effect": "PERMIT"
       }],
       "priority": 1000,
       "is_active": true
     }' \
     http://localhost:3000/api/v1/policies
```

### Audit and Compliance

#### Access Log Analysis

```bash
# Generate compliance reports
curl -H "Authorization: Bearer $AUDIT_TOKEN" \
     "http://localhost:3000/api/v1/access-control/logs/statistics?from_date=$(date -d '30 days ago' -Iseconds)&group_by=user" > monthly_access_report.json

# Identify suspicious access patterns
curl -H "Authorization: Bearer $AUDIT_TOKEN" \
     "http://localhost:3000/api/v1/access-control/logs?decision=DENY&limit=100" | \
     jq '.logs | group_by(.user_id) | map(select(length > 10)) | map({user_id: .[0].user_id, failed_attempts: length})'

# Track policy changes
grep "policy_update" logs/access_control.log | \
  jq -r '[.timestamp, .user_id, .policy_id, .action] | @csv'
```

#### Security Monitoring

```bash
# Monitor for privilege escalation attempts
curl -H "Authorization: Bearer $SECURITY_TOKEN" \
     "http://localhost:3000/api/v1/access-control/logs?action=admin&decision=DENY" | \
     jq '.logs[] | select(.rbac_result == "DENY" and .abac_result == "PERMIT")'

# Check for policy bypass attempts
curl -H "Authorization: Bearer $SECURITY_TOKEN" \
     "http://localhost:3000/api/v1/access-control/logs" | \
     jq '.logs[] | select(.evaluation_time_ms > 500)' # Unusually long evaluations
```

### Incident Response

#### Security Incident Procedures

1. **Immediate Response**
   ```bash
   # Switch to RBAC-only mode to limit attack surface
   curl -X PUT \
        -H "Authorization: Bearer $EMERGENCY_TOKEN" \
        -d '{"mode": "rbac_only"}' \
        http://localhost:3000/api/v1/access-control/config/mode
   
   # Disable suspicious policies
   curl -X PUT \
        -H "Authorization: Bearer $EMERGENCY_TOKEN" \
        -d '{"is_active": false}' \
        http://localhost:3000/api/v1/policies/suspicious-policy-id
   ```

2. **Investigation**
   ```bash
   # Collect access logs for analysis
   curl -H "Authorization: Bearer $INCIDENT_TOKEN" \
        "http://localhost:3000/api/v1/access-control/logs?from_date=$(date -d '24 hours ago' -Iseconds)" > incident_logs.json
   
   # Export system configuration
   curl -H "Authorization: Bearer $INCIDENT_TOKEN" \
        http://localhost:3000/api/v1/access-control/config > incident_config.json
   ```

3. **Recovery**
   ```bash
   # Restore from known good configuration
   curl -X PUT \
        -H "Authorization: Bearer $RECOVERY_TOKEN" \
        -d @known_good_config.json \
        http://localhost:3000/api/v1/access-control/config
   
   # Gradually re-enable features
   curl -X PUT \
        -H "Authorization: Bearer $RECOVERY_TOKEN" \
        -d '{"mode": "hybrid"}' \
        http://localhost:3000/api/v1/access-control/config/mode
   ```

## Backup and Recovery

### Database Backup

#### Automated Backup Script

```bash
#!/bin/bash
# backup_access_control.sh

BACKUP_DIR="/backups/access_control"
DATE=$(date +%Y%m%d_%H%M%S)
DB_FILE="data/sqlite.db"

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Backup database
cp "$DB_FILE" "$BACKUP_DIR/sqlite_$DATE.db"

# Backup configuration
curl -H "Authorization: Bearer $BACKUP_TOKEN" \
     http://localhost:3000/api/v1/access-control/config > "$BACKUP_DIR/config_$DATE.json"

# Export policies
curl -H "Authorization: Bearer $BACKUP_TOKEN" \
     http://localhost:3000/api/v1/policies > "$BACKUP_DIR/policies_$DATE.json"

# Compress and cleanup old backups
gzip "$BACKUP_DIR/sqlite_$DATE.db"
find "$BACKUP_DIR" -name "*.gz" -mtime +30 -delete

echo "Backup completed: $DATE"
```

#### Backup Verification

```bash
# Verify backup integrity
gunzip -t "/backups/access_control/sqlite_$DATE.db.gz"

# Test backup restoration in test environment
cp "/backups/access_control/sqlite_$DATE.db.gz" /tmp/
gunzip "/tmp/sqlite_$DATE.db.gz"
sqlite3 "/tmp/sqlite_$DATE.db" "PRAGMA integrity_check;"
```

### Recovery Procedures

#### Point-in-Time Recovery

```bash
# 1. Stop the application
sudo systemctl stop your-app-service

# 2. Backup current state
cp data/sqlite.db data/sqlite.db.pre_recovery

# 3. Restore from backup
cp /backups/access_control/sqlite_YYYYMMDD_HHMMSS.db data/sqlite.db

# 4. Restore configuration
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d @/backups/access_control/config_YYYYMMDD_HHMMSS.json \
     http://localhost:3000/api/v1/access-control/config

# 5. Start the application
sudo systemctl start your-app-service

# 6. Verify recovery
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
     http://localhost:3000/api/v1/access-control/health
```

#### Disaster Recovery

```bash
# Full system recovery from backups
#!/bin/bash

BACKUP_DATE="20240115_143000"
BACKUP_DIR="/backups/access_control"

# Stop services
sudo systemctl stop your-app-service

# Restore database
gunzip -c "$BACKUP_DIR/sqlite_$BACKUP_DATE.db.gz" > data/sqlite.db

# Restore application code (if needed)
git checkout production-stable
cargo build --release

# Start services
sudo systemctl start your-app-service

# Wait for startup
sleep 30

# Restore configuration
curl -X PUT \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d @"$BACKUP_DIR/config_$BACKUP_DATE.json" \
     http://localhost:3000/api/v1/access-control/config

# Restore policies
jq -r '.[]' "$BACKUP_DIR/policies_$BACKUP_DATE.json" | \
while read policy; do
  curl -X POST \
       -H "Authorization: Bearer $ADMIN_TOKEN" \
       -H "Content-Type: application/json" \
       -d "$policy" \
       http://localhost:3000/api/v1/policies
done

# Verify recovery
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
     http://localhost:3000/api/v1/access-control/health
```

## Maintenance Procedures

### Regular Maintenance Tasks

#### Daily Tasks

```bash
# Check system health
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     http://localhost:3000/api/v1/access-control/health

# Review error logs
grep ERROR logs/access_control.log | tail -20

# Check cache performance
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     http://localhost:3000/api/v1/access-control/health/statistics | \
     jq '.evaluation_stats.cache_hit_rate'
```

#### Weekly Tasks

```bash
# Analyze access patterns
curl -H "Authorization: Bearer $ANALYST_TOKEN" \
     "http://localhost:3000/api/v1/access-control/logs/statistics?from_date=$(date -d '7 days ago' -Iseconds)" > weekly_stats.json

# Review policy effectiveness
jq '.top_denied_resources' weekly_stats.json

# Database maintenance
sqlite3 data/sqlite.db "PRAGMA optimize; VACUUM;"
```

#### Monthly Tasks

```bash
# Generate compliance report
curl -H "Authorization: Bearer $AUDIT_TOKEN" \
     "http://localhost:3000/api/v1/access-control/logs/statistics?from_date=$(date -d '30 days ago' -Iseconds)" > monthly_compliance_report.json

# Review and update policies
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
     http://localhost:3000/api/v1/policies | \
     jq '.[] | select(.updated_at < (now - 86400*30))'

# Performance analysis
curl -H "Authorization: Bearer $MONITOR_TOKEN" \
     http://localhost:3000/api/v1/access-control/health/statistics > monthly_performance.json
```

### Database Maintenance

#### Log Cleanup

```sql
-- Clean up old access evaluation logs (keep 90 days)
DELETE FROM access_evaluation_log 
WHERE created_at < datetime('now', '-90 days');

-- Clean up old attribute history (keep 1 year)
DELETE FROM user_attribute 
WHERE valid_until IS NOT NULL 
  AND valid_until < datetime('now', '-365 days');
```

#### Index Maintenance

```sql
-- Rebuild indexes for optimal performance
REINDEX idx_access_evaluation_log_user_id;
REINDEX idx_access_evaluation_log_created_at;
REINDEX idx_user_attribute_user_id;

-- Analyze table statistics
ANALYZE access_evaluation_log;
ANALYZE user_attribute;
ANALYZE resource_attribute;
ANALYZE abac_policy;
```

### Cache Maintenance

```bash
# Clear cache if needed
curl -X POST \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     http://localhost:3000/api/v1/access-control/cache/clear

# Warm up cache with common queries
curl -X POST \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     http://localhost:3000/api/v1/access-control/cache/warmup
```

## Best Practices

### Policy Design

1. **Keep Policies Simple**: Avoid overly complex conditions
2. **Use Descriptive Names**: Make policy purpose clear
3. **Document Thoroughly**: Include detailed descriptions
4. **Test Extensively**: Validate with realistic scenarios
5. **Version Control**: Track policy changes
6. **Regular Review**: Audit policies for relevance

### Performance Optimization

1. **Enable Caching**: Use appropriate TTL values
2. **Monitor Metrics**: Track evaluation times via Prometheus endpoint
3. **Optimize Queries**: Ensure proper database indexes (see migration guide)
4. **Batch Operations**: Group related evaluations
5. **Circuit Breakers**: Implement failure protection
6. **Graceful Degradation**: Plan for component failures
7. **Use Optimized Queries**: Leverage `user_type_permission` table for faster lookups

### Security Considerations

1. **Principle of Least Privilege**: Grant minimal necessary access
2. **Defense in Depth**: Use multiple security layers
3. **Regular Audits**: Review access patterns and policies
4. **Secure Defaults**: Deny access by default
5. **Input Validation**: Validate all policy inputs
6. **Encryption**: Protect sensitive attribute data
7. **Configuration-Based Security**: Use `SUPER_ADMIN_ROLES` instead of hardcoded checks
8. **Log Redaction**: Configure `LOG_REDACTION_FIELDS` to protect sensitive data
9. **MFA for Admin**: Enable `REQUIRE_MFA_FOR_ADMIN` in production

### Operational Excellence

1. **Comprehensive Monitoring**: Track all key metrics via Prometheus
2. **Automated Backups**: Regular, tested backups
3. **Documentation**: Keep procedures up to date
4. **Training**: Ensure team understands system
5. **Incident Response**: Have clear procedures
6. **Change Management**: Control policy deployments
7. **Error Context**: Use structured error responses for debugging
8. **Request Correlation**: Track `request_id` across logs and errors

### Troubleshooting Guidelines

1. **Check Health First**: Always start with system health endpoint
2. **Review Logs**: Look for error patterns with structured context
3. **Check Metrics**: Review Prometheus metrics for anomalies
4. **Test Incrementally**: Isolate issues step by step
5. **Use Rollback**: Don't hesitate to revert changes
6. **Document Issues**: Track problems and solutions
7. **Learn and Improve**: Update procedures based on incidents
8. **Correlate Errors**: Use `request_id` to trace errors across components

### New Features and Migration

#### Using Enhanced Error Handling

All errors now include structured context. When troubleshooting:

```bash
# Search logs for specific request
grep "req_abc123" logs/access_control.log

# Find all database errors
grep "DatabaseError" logs/access_control.log | jq '.context'

# Identify timeout patterns
grep "EvaluationTimeout" logs/access_control.log | \
  jq -r '[.context.operation, .context.additional_data.component] | @csv'
```

#### Monitoring Performance Metrics

```bash
# Check current metrics
curl http://localhost:3000/api/v1/access-control/metrics

# Monitor specific metric
watch -n 5 'curl -s http://localhost:3000/api/v1/access-control/metrics | \
  grep "access_control_cache_hit_rate"'

# Export metrics to file for analysis
curl http://localhost:3000/api/v1/access-control/metrics > metrics_$(date +%Y%m%d_%H%M%S).txt
```

#### Configuring Security Settings

```bash
# Update security configuration
cat >> .env <<EOF
SUPER_ADMIN_ROLES=super_admin,system_admin,security_admin
SUPER_ADMIN_USER_TYPE_IDS=1,2
REQUIRE_MFA_FOR_ADMIN=true
ALLOWED_PERMISSION_PATTERNS=^[a-z_]+\.[a-z_]+$,^admin\.[a-z_]+\.[a-z_]+$
LOG_REDACTION_FIELDS=password,token,secret,api_key,ssn,credit_card
EOF

# Restart service to apply changes
sudo systemctl restart your-app-service

# Verify configuration
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
     http://localhost:3000/api/v1/access-control/config
```

#### Using Optimized Database Queries

The system now uses the `user_type_permission` mapping table for faster queries:

```bash
# Verify table exists
sqlite3 data/sqlite.db "SELECT COUNT(*) FROM user_type_permission;"

# Check index usage
sqlite3 data/sqlite.db "EXPLAIN QUERY PLAN 
  SELECT p.* FROM permission p
  INNER JOIN user_type_permission utp ON p.id = utp.permission_id
  WHERE utp.user_type_id = 1;"

# Monitor query performance
grep "find_by_user_type_id_optimized" logs/access_control.log | \
  jq '.context.additional_data.query_duration_ms' | \
  awk '{sum+=$1; count++} END {print "Average:", sum/count, "ms"}'
```

## Conclusion

This operations guide provides the foundation for successfully managing the Hybrid Access Control System in production. Regular adherence to these procedures will ensure optimal performance, security, and reliability.

Key takeaways:
- Monitor system health continuously
- Maintain comprehensive backups
- Follow security best practices
- Keep documentation current
- Plan for failure scenarios
- Optimize based on real-world usage

For additional support or questions not covered in this guide, contact the development team or refer to the troubleshooting section.