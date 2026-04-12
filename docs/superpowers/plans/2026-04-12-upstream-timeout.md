# Upstream Timeout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add route-level upstream timeouts with `504 Gateway Timeout` responses for slow backends.

**Architecture:** Keep timeout policy on the route itself and enforce it in `upstream::send`, where the gateway already owns the request lifecycle. Surface timeout as a dedicated gateway error so `route` can map it to `504`.

**Tech Stack:** Rust, Tokio, Hyper, Hyper Util

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse/default tests for `timeout_ms`**
- [ ] **Step 2: Add a slow-upstream integration test**
- [ ] **Step 3: Run focused timeout tests and verify red**

### Task 2: Implement Timeout Handling

**Files:**
- Modify: `src/config.rs`
- Modify: `src/entity.rs`
- Modify: `src/upstream.rs`
- Modify: `src/route.rs`

- [ ] **Step 1: Add `timeout_ms` to route config**
- [ ] **Step 2: Add a timeout-specific gateway error**
- [ ] **Step 3: Wrap upstream send in `tokio::time::timeout`**
- [ ] **Step 4: Map timeout errors to `504 Gateway Timeout`**
- [ ] **Step 5: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/entity.rs`
- Modify: `src/upstream.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-upstream-timeout-design.md docs/superpowers/plans/2026-04-12-upstream-timeout.md src/config.rs src/entity.rs src/upstream.rs src/route.rs tests/gateway_build.rs
git commit -m "feat: add route-level upstream timeout handling"
```
