# Management Metrics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a built-in `/__gateway/metrics` endpoint that exposes per-instance JSON request metrics.

**Architecture:** Keep metrics state in an explicit per-gateway collector passed through bootstrap and connection handling, then snapshot that collector through the existing management endpoint layer.

**Tech Stack:** Rust, Tokio, Hyper, Serde, Serde JSON

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add integration test for request metric snapshots**
- [ ] **Step 2: Add integration test for gateway error metric snapshots**
- [ ] **Step 3: Run focused tests and verify red**

### Task 2: Implement Metrics Collection

**Files:**
- Create: `src/metrics.rs`
- Modify: `src/lib.rs`
- Modify: `src/main.rs`
- Modify: `src/management.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add a per-instance metrics collector and snapshot types**
- [ ] **Step 2: Pass the collector through gateway bootstrap and test helpers**
- [ ] **Step 3: Record request outcomes and durations in `route`**
- [ ] **Step 4: Serve `/__gateway/metrics` from the management layer**
- [ ] **Step 5: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Create: `src/metrics.rs`
- Modify: `src/lib.rs`
- Modify: `src/main.rs`
- Modify: `src/management.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-management-metrics-design.md docs/superpowers/plans/2026-04-12-management-metrics.md src/metrics.rs src/lib.rs src/main.rs src/management.rs src/route.rs tests/gateway_build.rs
git commit -m "feat: add built-in management metrics endpoint"
```
