# Management Endpoints Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add built-in health and route-listing endpoints served directly by the gateway.

**Architecture:** Intercept management paths before throttle and upstream routing, and return JSON bodies from a lightweight in-process management module. Extend `GatewayBody` so the gateway can emit local JSON responses as well as proxied upstream bodies.

**Tech Stack:** Rust, Tokio, Hyper, Serde, Serde JSON

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add integration test for `GET /__gateway/health` without API key**
- [ ] **Step 2: Add integration test for `GET /__gateway/routes` returning configured routes**
- [ ] **Step 3: Run focused tests and verify red**

### Task 2: Implement Management Endpoints

**Files:**
- Create: `src/management.rs`
- Modify: `src/entity.rs`
- Modify: `src/lib.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add a direct-bytes response body variant**
- [ ] **Step 2: Implement management path handling and JSON serialization**
- [ ] **Step 3: Intercept management requests before throttle and upstream routing**
- [ ] **Step 4: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Create: `src/management.rs`
- Modify: `src/entity.rs`
- Modify: `src/lib.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-management-endpoints-design.md docs/superpowers/plans/2026-04-12-management-endpoints.md src/management.rs src/entity.rs src/lib.rs src/route.rs tests/gateway_build.rs
git commit -m "feat: add built-in management endpoints"
```
