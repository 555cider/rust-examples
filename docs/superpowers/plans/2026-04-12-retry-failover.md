# Retry Failover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retry transport failures per route and fail over to the next upstream.

**Architecture:** Buffer the incoming request body once, keep a reusable request template, and let the upstream layer sequence attempts from a single round-robin starting point.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for `retry_attempts`**
- [ ] **Step 2: Add failover integration test for connection failure**
- [ ] **Step 3: Add failover integration test for upstream timeout**
- [ ] **Step 4: Run focused tests and verify red**

### Task 2: Implement Request Rebuild And Retry

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `src/upstream.rs`
- Modify: `src/connector.rs`

- [ ] **Step 1: Add route-level `retry_attempts` config**
- [ ] **Step 2: Buffer incoming request bodies into a reusable template**
- [ ] **Step 3: Move per-attempt request building into the upstream layer**
- [ ] **Step 4: Retry retryable transport failures across upstream candidates**
- [ ] **Step 5: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `src/upstream.rs`
- Modify: `src/connector.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-retry-failover-design.md docs/superpowers/plans/2026-04-12-retry-failover.md src/config.rs src/route.rs src/upstream.rs src/connector.rs tests/gateway_build.rs
git commit -m "feat: add route-level retry failover"
```
