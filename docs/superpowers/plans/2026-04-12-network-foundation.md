# Network Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the gateway bootstrap and upstream transport path so phase 1 supports configured listen addresses, hostname-based upstreams, and HTTPS upstream connections.

**Architecture:** Move socket and transport concerns out of routing into a dedicated connector layer. Keep route matching and request rewriting focused in `route`, while `upstream` becomes a small dispatcher that chooses protocol-specific Hyper handshakes over plain or TLS sockets.

**Tech Stack:** Rust, Tokio, Hyper, Hyper Util, Serde YAML, SQLite throttle state, Rustls/Tokio-Rustls for HTTPS upstreams

---

### Task 1: Redesign Config And Bootstrap

**Files:**
- Modify: `src/config.rs`
- Modify: `src/main.rs`
- Modify: `config.yaml`
- Test: `src/config.rs`

- [ ] **Step 1: Write the failing config tests**

```rust
#[test]
fn parses_server_listen_and_routes_alias() {
    let config = serde_yaml::from_str::<GatewayConfig>(
        r#"
server:
  listen: 127.0.0.1:9090
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    path: /
"#,
    )
    .expect("config should parse");

    assert_eq!(config.server.listen, "127.0.0.1:9090");
    assert_eq!(config.routes[0].authority.host, "localhost");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test config::tests::parses_server_listen_and_routes_alias -- --exact`
Expected: FAIL because `GatewayConfig` does not expose `server` and `routes`

- [ ] **Step 3: Write the minimal implementation**

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GatewayConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(alias = "route")]
    pub routes: Arc<[Route]>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen: String,
}
```

- [ ] **Step 4: Run focused tests**

Run: `cargo test config::tests -- --nocapture`
Expected: PASS for config parsing tests

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/main.rs config.yaml
git commit -m "refactor: redesign config and bootstrap for network foundation"
```

### Task 2: Add Connector Abstraction And Hostname Resolution

**Files:**
- Create: `src/connector.rs`
- Modify: `src/lib.rs`
- Modify: `src/route.rs`
- Modify: `src/upstream.rs`
- Test: `src/connector.rs`
- Test: `tests/gateway_build.rs`

- [ ] **Step 1: Write the failing hostname integration test**

```rust
#[tokio::test]
async fn supports_http1_gateway_to_http1_hostname_upstream() {
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: "localhost".to_string(),
                port: upstream.addr.port().to_string(),
            },
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test supports_http1_gateway_to_http1_hostname_upstream -- --exact`
Expected: FAIL because upstream resolution only accepts literal IP addresses

- [ ] **Step 3: Write the minimal implementation**

```rust
pub async fn connect(route: &config::Route) -> Result<UpstreamStream, entity::GatewayError> {
    let addr = format!("{}:{}", route.authority.host, route.authority.port);
    let stream = tokio::net::TcpStream::connect(addr).await?;
    Ok(UpstreamStream::Plain(stream))
}
```

- [ ] **Step 4: Run focused tests**

Run: `cargo test supports_http1_gateway_to_http1_hostname_upstream -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/connector.rs src/route.rs src/upstream.rs tests/gateway_build.rs
git commit -m "feat: add connector abstraction with hostname resolution"
```

### Task 3: Add HTTPS Upstream Transport

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src/connector.rs`
- Modify: `src/entity.rs`
- Modify: `src/upstream.rs`
- Test: `tests/gateway_build.rs`

- [ ] **Step 1: Write the failing HTTPS integration test**

```rust
#[tokio::test]
async fn supports_http1_gateway_to_https_upstream() {
    let upstream = spawn_https_http1_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "https".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: "localhost".to_string(),
                port: upstream.addr.port().to_string(),
            },
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"https-http1"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test supports_http1_gateway_to_https_upstream -- --exact`
Expected: FAIL because the gateway still opens plain TCP sockets to `https` routes

- [ ] **Step 3: Write the minimal implementation**

```rust
let config = rustls::ClientConfig::builder()
    .with_root_certificates(root_store)
    .with_no_client_auth();
let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
let tls_stream = connector.connect(server_name, tcp_stream).await?;
```

- [ ] **Step 4: Run focused tests**

Run: `cargo test supports_http1_gateway_to_https_upstream -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/connector.rs src/entity.rs src/upstream.rs tests/gateway_build.rs
git commit -m "feat: add upstream https transport support"
```

### Task 4: Full Verification

**Files:**
- Test: `src/config.rs`
- Test: `tests/gateway_build.rs`

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: PASS for unit and integration tests

- [ ] **Step 2: Review for regression risk**

Check:
- mixed route specificity still works
- trace header propagation still works
- throttle behavior remains unchanged

- [ ] **Step 3: Record final status**

Document any unsupported combinations that remain out of scope for phase 1, especially inbound TLS and HTTP/2 over TLS.
