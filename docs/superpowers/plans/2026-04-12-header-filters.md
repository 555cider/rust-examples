# Header Filters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add route-level static request and response header filters.

**Architecture:** Keep header filter config reusable and apply it at the two natural boundaries that already exist: request template preparation in `route`, and response assembly in `upstream`.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for header filters**
- [ ] **Step 2: Add unit test for filter application order**
- [ ] **Step 3: Add integration tests for request and response header filtering**
- [ ] **Step 4: Run focused tests and verify red**

### Task 2: Implement Filter Support

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `src/upstream.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add reusable `HeaderFilterConfig` to route config**
- [ ] **Step 2: Apply request filters while preparing the forwarded request template**
- [ ] **Step 3: Apply response filters before returning upstream responses**
- [ ] **Step 4: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `src/upstream.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-header-filters-design.md docs/superpowers/plans/2026-04-12-header-filters.md src/config.rs src/route.rs src/upstream.rs tests/gateway_build.rs
git commit -m "feat: add request and response header filters"
```
