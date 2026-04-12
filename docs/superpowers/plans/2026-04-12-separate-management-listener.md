# Separate Management Listener Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow built-in management endpoints to bind to a dedicated listener without breaking existing shared-listener behavior.

**Architecture:** Keep management routing explicit. The main listener decides whether built-in management interception is enabled. When a dedicated management listener exists, it serves built-in endpoints only and returns `404` for everything else.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for `server.management.listen`**
- [ ] **Step 2: Add integration test for dedicated management listener health access**
- [ ] **Step 3: Add integration test proving main listener no longer intercepts management paths**
- [ ] **Step 4: Add integration test proving non-management paths return `404` on the management listener**
- [ ] **Step 5: Run focused tests and verify red**

### Task 2: Implement Dedicated Management Listener

**Files:**
- Modify: `src/config.rs`
- Modify: `src/main.rs`
- Modify: `src/management.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add management listen config**
- [ ] **Step 2: Gate built-in management interception on the listener mode**
- [ ] **Step 3: Bind and serve the optional management listener**
- [ ] **Step 4: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/main.rs`
- Modify: `src/management.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-separate-management-listener-design.md docs/superpowers/plans/2026-04-12-separate-management-listener.md src/config.rs src/main.rs src/management.rs src/route.rs tests/gateway_build.rs
git commit -m "feat: add dedicated management listener support"
```
