# Circuit Breaker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a minimal route-level circuit breaker that fails fast after repeated upstream transport failures.

**Architecture:** Keep breaker decisions in `upstream`, where retry exhaustion and success/failure outcomes already converge. Expose only config and a dedicated error variant to the rest of the gateway.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for `circuit_breaker`**
- [ ] **Step 2: Add a unit test proving `CircuitOpen` maps to `503 Service Unavailable`**
- [ ] **Step 3: Add an integration test for fail-open-fast-recover behavior**
- [ ] **Step 4: Run focused tests and verify red**

### Task 2: Implement Circuit Breaker Support

**Files:**
- Modify: `src/config.rs`
- Modify: `src/entity.rs`
- Modify: `src/route.rs`
- Modify: `src/upstream.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add optional `CircuitBreakerConfig` to route config**
- [ ] **Step 2: Add explicit `GatewayError::CircuitOpen`**
- [ ] **Step 3: Track consecutive route failures and open-until timestamps in `upstream`**
- [ ] **Step 4: Reset breaker state on successful upstream responses**
- [ ] **Step 5: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/entity.rs`
- Modify: `src/route.rs`
- Modify: `src/upstream.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-circuit-breaker-design.md docs/superpowers/plans/2026-04-12-circuit-breaker.md src/config.rs src/entity.rs src/route.rs src/upstream.rs tests/gateway_build.rs
git commit -m "feat: add route-level circuit breaker"
```
