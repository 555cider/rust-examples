# auth-server

Standalone Rust authentication and authorization server for a single-tenant, single-customer deployment.

## Features

- OAuth 2.1-style flows
  - Authorization Code + PKCE
  - Client Credentials
  - Refresh token rotation with reuse detection
- OpenID Connect
  - `/.well-known/openid-configuration`
  - `/jwks.json`
  - `/userinfo`
  - ID token issuance
- Local user accounts with Argon2id password hashing
- Session-cookie login/logout
- JWT access tokens signed with Ed25519 / EdDSA
- RBAC + DB-backed ABAC policy evaluation
- Admin JSON APIs for users, clients, roles, permissions, and policies
- Audit logging for security-relevant events

## Repository Layout

- `src/app.rs`
  - application bootstrap and router assembly
- `src/http/`
  - transport layer and route handlers
- `src/identity/`
  - local credentials, sessions, and user lookups
- `src/oauth.rs`
  - OAuth client/auth code/refresh token domain logic
- `src/token.rs`
  - JWT signing, verification, and JWKS material
- `src/policy.rs`
  - RBAC/ABAC evaluation
- `src/admin.rs`
  - admin CRUD domain logic
- `src/audit.rs`
  - audit event persistence
- `migrations/`
  - PostgreSQL schema
- `tests/`
  - integration and regression coverage

This structure is pragmatic for the current scope. For a single binary with clear module boundaries, it is a good shape. It is not a universal "best practice" for every team or scale, but it is appropriate for this repository size and responsibility split.

## Local Development

Start PostgreSQL:

```bash
docker compose up -d postgres
```

Set environment variables:

```bash
cp .env.example .env
set -a
source .env
set +a
```

Run the server:

```bash
cargo run
```

By default the server listens on `127.0.0.1:3000`.

## Verification

Run the full test suite:

```bash
DATABASE_URL=postgres://auth_server:auth_server@127.0.0.1:25432/auth_server cargo test
```

Check formatting:

```bash
cargo fmt --check
```

## Key Endpoints

- `GET /healthz`
- `POST /login`
- `POST /logout`
- `GET /authorize`
- `POST /token`
- `GET /.well-known/openid-configuration`
- `GET /jwks.json`
- `GET /userinfo`
- `POST /policy/evaluate`
- `GET|POST|PATCH|DELETE /admin/users`
- `GET|POST|PATCH|DELETE /admin/clients`
- `GET|POST|PATCH|DELETE /admin/roles`
- `GET|POST|PATCH|DELETE /admin/permissions`
- `GET|POST|PATCH|DELETE /admin/policies`

Admin APIs require a logged-in user with the `admin:manage` permission.

## Notes

- Loopback/local runs can use an ephemeral signing key if `AUTH_SERVER_SIGNING_KEY_PATH` is unset.
- Non-local deployments should provide a PKCS#8 Ed25519 private key path.
- OIDC discovery and JWKS responses include `Cache-Control: public, max-age=300`.

## Design Reference

- [docs/design.md](docs/design.md)
