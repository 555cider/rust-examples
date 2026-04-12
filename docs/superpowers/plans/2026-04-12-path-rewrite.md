# Path Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional route prefix rewriting while preserving existing route matching and transport behavior.

**Architecture:** Keep `path` as the matching prefix and derive a rewritten `path_and_query` once, right after route selection, so retries and load balancing keep using the same transformed upstream path.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for `rewrite_prefix`**
- [ ] **Step 2: Add route unit tests for prefix rewrite behavior**
- [ ] **Step 3: Add integration test proving upstream receives the rewritten path**
- [ ] **Step 4: Run focused tests and verify red**

### Task 2: Implement Rewrite Support

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`

- [ ] **Step 1: Add optional `rewrite_prefix` to route config**
- [ ] **Step 2: Rewrite the selected request path-and-query before storing the request template**
- [ ] **Step 3: Validate `rewrite_prefix` format**
- [ ] **Step 4: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-path-rewrite-design.md docs/superpowers/plans/2026-04-12-path-rewrite.md src/config.rs src/route.rs tests/gateway_build.rs
git commit -m "feat: add route prefix rewrite support"
```
