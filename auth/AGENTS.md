# AGENTS.md

## Repository Intent

`auth/` is the auth server of `rust-examples`. It is a standalone crate: do not share code with `admin/` (the older admin dashboard example).

## First Read

Before making code changes, read these files in order:

1. `README.md`
2. `docs/design.md`

## Working Rules

- Keep commits small and task-scoped.
- Prefer secure defaults over convenience.
- Do not add multi-tenancy in `v1`.
- Do not add password grant.
- Use PostgreSQL, local accounts, Ed25519/EdDSA JWT signing, RBAC, and DB-backed ABAC.
- Keep token claims small. Fine-grained authorization belongs in the policy layer.
