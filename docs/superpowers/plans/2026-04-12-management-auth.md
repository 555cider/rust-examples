# Management Auth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional bearer-token protection for built-in management endpoints.

**Architecture:** Keep auth policy small and explicit: a single static token in server config, checked centrally inside the management handler before any endpoint-specific response is built.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for `server.management.auth_token`**
- [ ] **Step 2: Add integration test for unauthorized management access**
- [ ] **Step 3: Add integration test for authorized management access**
- [ ] **Step 4: Run focused tests and verify red**

### Task 2: Implement Management Auth

**Files:**
- Modify: `src/config.rs`
- Modify: `src/main.rs`
- Modify: `src/management.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add management auth config types**
- [ ] **Step 2: Pass management config through gateway bootstrap**
- [ ] **Step 3: Check bearer auth in management handling**
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
git add docs/superpowers/specs/2026-04-12-management-auth-design.md docs/superpowers/plans/2026-04-12-management-auth.md src/config.rs src/main.rs src/management.rs src/route.rs tests/gateway_build.rs
git commit -m "feat: add management bearer token auth"
```
