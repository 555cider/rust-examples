# rust-examples

Rust sample workspace. Each directory is an independent crate (no shared Cargo workspace).

| Directory | What | Status |
|---|---|---|
| `auth/` | OAuth 2.1 / OIDC auth server: PKCE, refresh rotation, EdDSA JWT, RBAC + ABAC, audit log (Axum, PostgreSQL) | maintained |
| `gateway/` | API gateway with Spring Cloud Gateway-style YAML routes, DNS / Consul / Eureka discovery, retry, circuit breaker, rate limit (hyper) | maintained |
| `admin/` | Admin dashboard with RBAC / ABAC policy management and web UI (Axum, Tera, SQLite) | maintained |
| `actix-diesel/` | CRUD API with Actix Web and Diesel | experiment |
| `axum-sqlx/` | CRUD API with Axum and SQLx | experiment |
| `candle-llm/` | Local LLM inference with Candle | experiment |
| `hyper-webtransport/` | WebTransport over HTTP/3 (vendored hyperium/h3) | experiment |
| `meilisearch/` | Importer, indexer, and viewer around Meilisearch | experiment |
| `socks5-proxy/` | SOCKS5 proxy server | experiment |
| `tokio-websocket/` | WebSocket with tokio-tungstenite | experiment |

Experiments are early prototypes with old dependency versions and are not maintained.

## History

`auth/`, `gateway/`, and `admin/` were separate repositories (`auth-server`, `hyper-gateway`, `rust-admin-server`) and were merged here with their commit history.
