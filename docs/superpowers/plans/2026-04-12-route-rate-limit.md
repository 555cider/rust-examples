# Route Rate Limit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current hard-coded throttle with optional route-level in-memory rate limiting.

**Architecture:** Resolve the route first, then apply a per-instance fixed-window limiter keyed by route path and header value. Keep the limiter local to the gateway process and leave management endpoints outside the limiter path.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for `rate_limit`**
- [ ] **Step 2: Add integration test for unlimited route access without API key**
- [ ] **Step 3: Add integration test for limited route missing key**
- [ ] **Step 4: Add integration test for limit exceeded**
- [ ] **Step 5: Run focused tests and verify red**

### Task 2: Implement Route Rate Limiting

**Files:**
- Modify: `src/config.rs`
- Modify: `src/main.rs`
- Modify: `src/route.rs`
- Modify: `src/throttle.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add `RateLimitConfig` to route config**
- [ ] **Step 2: Replace SQLite throttle internals with an in-memory limiter store**
- [ ] **Step 3: Apply the limiter after route resolution and before request body preparation**
- [ ] **Step 4: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/main.rs`
- Modify: `src/route.rs`
- Modify: `src/throttle.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-route-rate-limit-design.md docs/superpowers/plans/2026-04-12-route-rate-limit.md src/config.rs src/main.rs src/route.rs src/throttle.rs tests/gateway_build.rs
git commit -m "feat: add route-level in-memory rate limiting"
```
