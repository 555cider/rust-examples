# SCG-Style YAML And DNS Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Accept a Spring Cloud Gateway-like route YAML subset and make hostname services discovery-backed through DNS round-robin.

**Architecture:** Keep runtime routing code centered on the current internal `Route` model. Add a conversion layer that translates SCG-style route definitions into that model, and add DNS-aware connector selection so service hostnames behave like discovery-backed upstream pools.

**Tech Stack:** Rust, Tokio, Hyper, Serde YAML

---

### Task 1: Add Failing Tests

**Files:**
- Modify: `src/config.rs`
- Modify: `src/connector.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse coverage for `spring.cloud.gateway.routes`**
- [ ] **Step 2: Add config rejection coverage for unsupported SCG predicates or filters**
- [ ] **Step 3: Add connector unit coverage for DNS address round-robin selection**
- [ ] **Step 4: Add integration tests for SCG-style route proxying and prefix rewrite**
- [ ] **Step 5: Run focused tests and verify red**

### Task 2: Implement SCG Route Conversion And DNS Discovery

**Files:**
- Modify: `src/config.rs`
- Modify: `src/connector.rs`
- Modify: `src/main.rs`
- Modify: `src/route.rs`
- Modify: `config.yaml`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add raw config parsing for SCG-style route definitions**
- [ ] **Step 2: Convert supported predicates, filters, and metadata into internal routes**
- [ ] **Step 3: Add DNS multi-address resolution and per-authority round-robin selection**
- [ ] **Step 4: Update sample config to SCG-style YAML**
- [ ] **Step 5: Re-run focused tests until green**

### Task 3: Verify And Commit

**Files:**
- Modify: `src/config.rs`
- Modify: `src/connector.rs`
- Modify: `src/main.rs`
- Modify: `src/route.rs`
- Modify: `config.yaml`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Run `cargo test`**
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-scg-yaml-and-dns-discovery-design.md docs/superpowers/plans/2026-04-12-scg-yaml-and-dns-discovery.md src/config.rs src/connector.rs src/main.rs src/route.rs config.yaml tests/gateway_build.rs
git commit -m "feat: add scg-style route config and dns-backed discovery"
```
