# Inbound TLS Termination Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional TLS termination for inbound client traffic without regressing existing plain HTTP traffic.

**Architecture:** Keep the accept loop in `main`, but move request serving to a generic `route` entry point that works over either a `TcpStream` or a Rustls TLS stream. Configure TLS with explicit server certificate and private key paths under `server.tls`.

**Tech Stack:** Rust, Tokio, Hyper, Hyper Util, Rustls, Tokio-Rustls, rcgen for integration tests

---

### Task 1: Add Failing HTTPS Client Test

**Files:**
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Write the failing integration test**

```rust
#[tokio::test]
async fn supports_https_client_to_http1_upstream() {
    let tls_fixture = create_localhost_tls_fixture();
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_tls_gateway_once(...).await;

    let (status, body) = send_https_http1_request(gateway.addr, &tls_fixture).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test supports_https_client_to_http1_upstream -- --exact`
Expected: FAIL because the gateway only serves plain TCP

### Task 2: Add Gateway TLS Config And Generic Serve Path

**Files:**
- Modify: `src/config.rs`
- Modify: `src/main.rs`
- Modify: `src/route.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add typed `server.tls` config**

```rust
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ServerTlsConfig {
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}
```

- [ ] **Step 2: Refactor `route` to serve generic I/O streams**

```rust
pub async fn serve_connection<T>(stream: T, ...) -> Result<(), entity::GatewayError>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
```

- [ ] **Step 3: Add TLS accept path in `main`**

```rust
let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
let stream = tls_acceptor.accept(gateway_stream).await?;
route::serve_connection(stream, routes, throttle_pool).await?;
```

- [ ] **Step 4: Re-run focused test**

Run: `cargo test supports_https_client_to_http1_upstream -- --exact`
Expected: PASS

### Task 3: Verify Regression Safety

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/gateway_build.rs`

- [ ] **Step 1: Add config parse test for `server.tls`**
- [ ] **Step 2: Run full suite**

Run: `cargo test`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-04-12-inbound-tls-termination-design.md docs/superpowers/plans/2026-04-12-inbound-tls-termination.md src/config.rs src/main.rs src/route.rs tests/gateway_build.rs
git commit -m "feat: add optional gateway tls termination"
```
