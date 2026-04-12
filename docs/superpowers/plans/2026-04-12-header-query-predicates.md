# Header And Query Predicates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add exact-match `headers` and `queries` route predicates.

**Architecture:** Keep predicate evaluation centralized in `route` so path, host, method, header, and query conditions are considered together before longest-prefix selection.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for `headers` and `queries`**
- [ ] **Step 2: Add route unit tests for exact header and query matching**
- [ ] **Step 3: Add integration tests for header- and query-based route selection**
- [ ] **Step 4: Run focused tests and verify red**

### Task 2: Implement Predicate Matching

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add `headers` and `queries` maps to route config**
- [ ] **Step 2: Evaluate header and query predicates during route selection**
- [ ] **Step 3: Count these predicates in route specificity tie-breaking**
- [ ] **Step 4: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-header-query-predicates-design.md docs/superpowers/plans/2026-04-12-header-query-predicates.md src/config.rs src/route.rs tests/gateway_build.rs
git commit -m "feat: add header and query route predicates"
```
