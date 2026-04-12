# Start Here

This repository was created from an approved design and implementation plan. It intentionally starts with docs first so implementation can begin cleanly in this repository instead of inside `rust-examples`.

## Current State

- Git repository initialized at `/home/my/dev/auth-server`
- No application code yet
- Design and execution plan copied into this repository

## v1 Product Boundary

- Single tenant
- Single customer deployment
- Local user accounts
- OAuth 2.1-style auth flows
- OpenID Connect
- JWT access tokens validated through JWKS
- RBAC + ABAC policy evaluation API
- PostgreSQL from day one

## Explicitly Out of Scope for v1

- Multi-tenancy
- External IdP federation
- Dynamic client registration
- Token introspection and revocation
- SCIM
- Admin UI

## Recommended Implementation Order

1. Task 1: repository scaffold and health endpoint
2. Task 2: PostgreSQL wiring and initial schema
3. Task 3: local identity and login
4. Task 4: token service, discovery, JWKS
5. Task 5: OAuth code flow, PKCE, client credentials, refresh rotation
6. Task 6: OIDC ID token and UserInfo
7. Task 7: RBAC, ABAC, `/policy/evaluate`
8. Task 8: admin APIs, audit logging, full verification

## Practical Next Move

Use the prompt in [prompts/continue-task-1.md](prompts/continue-task-1.md) and implement only Task 1 first. Commit after Task 1 completes and verifies cleanly.

