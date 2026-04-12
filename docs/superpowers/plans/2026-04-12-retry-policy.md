# Retry Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add route-level status-based retry and fixed backoff to the existing retry system.

**Architecture:** Keep retry decision-making inside `upstream`, where attempts already loop over authorities and replay buffered requests. Only route config and retry decision logic change in this phase.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for `retry_statuses` and `retry_backoff_ms`**
- [ ] **Step 2: Add integration test for retryable upstream status failover**
- [ ] **Step 3: Run focused tests and verify red**

### Task 2: Implement Retry Policy

**Files:**
- Modify: `src/config.rs`
- Modify: `src/upstream.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add retry status and backoff config fields**
- [ ] **Step 2: Retry configured upstream response statuses in `upstream`**
- [ ] **Step 3: Apply fixed backoff before retry attempts**
- [ ] **Step 4: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/upstream.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-retry-policy-design.md docs/superpowers/plans/2026-04-12-retry-policy.md src/config.rs src/upstream.rs tests/gateway_build.rs
git commit -m "feat: add status-based retry policy"
```
