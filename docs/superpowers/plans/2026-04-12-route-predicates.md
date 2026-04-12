# Route Predicates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `hosts` and `methods` route predicates while preserving longest-prefix selection.

**Architecture:** Keep route config declarative and move predicate evaluation into the route matching layer so path, host, and method are considered together before choosing the winning route.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for `hosts` and `methods`**
- [ ] **Step 2: Add route unit tests for host and method predicate matching**
- [ ] **Step 3: Add integration tests for host and method route selection**
- [ ] **Step 4: Run focused tests and verify red**

### Task 2: Implement Predicate Matching

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add optional `hosts` and `methods` to route config**
- [ ] **Step 2: Normalize request host extraction**
- [ ] **Step 3: Filter candidate routes by host and method before longest-prefix selection**
- [ ] **Step 4: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-route-predicates-design.md docs/superpowers/plans/2026-04-12-route-predicates.md src/config.rs src/route.rs tests/gateway_build.rs
git commit -m "feat: add host and method route predicates"
```
