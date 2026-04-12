# Registry-Backed Discovery Providers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `lb://service` routes use Consul or Eureka registry snapshots instead of DNS-only resolution.

**Architecture:** Keep the current runtime route model and explicit route matching flow. Add a discovery cache that is initialized once at startup, refreshed in the background, and consulted only for discovery-backed routes. This keeps static routing behavior unchanged while making `lb://service` actually registry-backed.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML, Reqwest

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `src/connector.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for Consul and Eureka discovery settings**
- [ ] **Step 2: Add config parse coverage for `lb://service` discovery metadata**
- [ ] **Step 3: Add unit coverage for discovery-backed instance rotation**
- [ ] **Step 4: Add integration tests for Consul-backed and Eureka-backed proxy routing**
- [ ] **Step 5: Add integration coverage proving registry refresh updates the chosen backend**
- [ ] **Step 6: Run focused tests and verify red**

### Task 2: Implement Discovery Cache And Providers

**Files:**
- Create: `src/discovery.rs`
- Modify: `src/config.rs`
- Modify: `src/connector.rs`
- Modify: `src/main.rs`
- Modify: `src/route.rs`
- Modify: `src/upstream.rs`
- Modify: `src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add discovery config types and route discovery metadata**
- [ ] **Step 2: Implement provider polling and snapshot storage in `src/discovery.rs`**
- [ ] **Step 3: Initialize discovery in `main` and thread the handle through request serving**
- [ ] **Step 4: Use discovery-backed authorities in `connector` and `upstream`**
- [ ] **Step 5: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/discovery.rs`
- Modify: `src/config.rs`
- Modify: `src/connector.rs`
- Modify: `src/main.rs`
- Modify: `src/route.rs`
- Modify: `src/upstream.rs`
- Modify: `src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo fmt`**
- [ ] **Step 2: Run `cargo test`**
- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-registry-discovery-providers-design.md docs/superpowers/plans/2026-04-12-registry-discovery-providers.md Cargo.toml src/discovery.rs src/config.rs src/connector.rs src/main.rs src/route.rs src/upstream.rs src/lib.rs tests/gateway_build.rs
git commit -m "feat: add registry-backed discovery providers"
```
