# AGENTS.md

## Repository Intent

This repository is the standalone home for the new auth server. Do not treat it as an extension of `rust-examples`.

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
