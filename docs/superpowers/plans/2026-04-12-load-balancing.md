# Static Round-Robin Load Balancing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional per-route static upstream pools and distribute requests round-robin across them.

**Architecture:** Keep the route model stable by preserving the existing primary `authority` and adding `upstreams` as extra backends. Select the backend in `route_request` so request rewriting and transport both use the same chosen authority.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse test for `upstreams`**
- [ ] **Step 2: Add round-robin integration test across two HTTP/1 upstreams**
- [ ] **Step 3: Run focused test and verify red**

### Task 2: Implement Upstream Pool Selection

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `src/upstream.rs`
- Modify: `src/connector.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add optional `upstreams` to route config**
- [ ] **Step 2: Select the effective authority in `route_request`**
- [ ] **Step 3: Carry the selected authority through request rewriting and transport**
- [ ] **Step 4: Re-run focused test until green**

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
git add docs/superpowers/specs/2026-04-12-load-balancing-design.md docs/superpowers/plans/2026-04-12-load-balancing.md src/config.rs src/route.rs src/upstream.rs src/connector.rs tests/gateway_build.rs
git commit -m "feat: add static round-robin load balancing"
```
