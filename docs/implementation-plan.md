# Dedicated Auth Server Implementation Plan

## Goal

Build `/home/my/dev/auth-server` into a production-oriented `v1` auth server with local identity, OAuth 2.1-style flows, OIDC, JWT/JWKS, RBAC, and DB-backed ABAC.

## Task Order

### Task 1: Scaffold the Repository

Deliver:

- `Cargo.toml`
- `src/main.rs`
- `src/lib.rs`
- `src/app.rs`
- `src/config.rs`
- `src/error.rs`
- `src/http/router.rs`
- `src/http/health.rs`
- `.env.example`
- `.gitignore`
- health endpoint test

Commit target:

- `chore: scaffold auth server repository`

### Task 2: Add PostgreSQL and Initial Schema

Deliver:

- `docker-compose.yml`
- SQL migrations
- DB pool wiring
- initial schema for users, credentials, clients, auth codes, refresh tokens, roles, permissions, policies, audit logs

Commit target:

- `feat: add auth server persistence schema`

### Task 3: Add Local Identity and Login

Deliver:

- local user/password model
- Argon2id password handling
- login/logout routes
- login tests

Commit target:

- `feat: add local identity and login routes`

### Task 4: Add Token Service, Discovery, and JWKS

Deliver:

- signing key loading
- JWT issuance
- `/.well-known/openid-configuration`
- `/jwks.json`

Commit target:

- `feat: add jwks and oidc discovery`

### Task 5: Add OAuth Flows

Deliver:

- `/authorize`
- `/token`
- Authorization Code + PKCE
- Client Credentials
- refresh token rotation

Commit target:

- `feat: add oauth code and client credentials flows`

### Task 6: Add OIDC Identity Endpoints

Deliver:

- ID token issuance
- `/userinfo`

Commit target:

- `feat: add oidc id token and userinfo`

### Task 7: Add RBAC, ABAC, and Policy API

Deliver:

- role/permission model
- policy engine
- `/policy/evaluate`

Commit target:

- `feat: add rbac and abac policy evaluation`

### Task 8: Add Admin APIs and Audit Logging

Deliver:

- admin CRUD surfaces for users, clients, roles, permissions, policies
- audit logging for security events
- full verification pass

Commit target:

- `feat: add admin apis and audit logging`

## Execution Rules

- Work task-by-task, not all at once
- Verify each task before committing
- Prefer TDD where practical
- Keep security choices aligned with the design document
- Do not widen `v1` scope while implementing

