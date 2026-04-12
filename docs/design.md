# Dedicated Auth Server Design

## Summary

Build a standalone Rust auth server for a single tenant and a single customer deployment. The service provides OAuth 2.1-style flows, OpenID Connect, local account authentication, JWT issuance, RBAC, and DB-backed ABAC policy evaluation.

## Goals

- Dedicated auth/authz service, separate from the old admin example
- OAuth 2.1-style authorization flows for browser and machine clients
- OIDC login support
- JWT access tokens validated through JWKS
- Fine-grained authorization through RBAC and ABAC
- Secure storage and token handling defaults

## Non-Goals

- Multi-tenant shared deployment
- Social login or external federation
- Dynamic client registration
- Token introspection and revocation in `v1`
- SCIM
- Admin UI in `v1`

## Architecture

Use a modular monolith with these internal modules:

- `identity`
- `oauth`
- `oidc`
- `token`
- `policy`
- `admin`
- `audit`
- `infra`

Deployment remains a single binary, but boundaries stay explicit.

## Protocol Scope

Supported `v1` flows:

- Authorization Code + PKCE
- Client Credentials
- Refresh Token rotation

Required endpoints:

- `/.well-known/openid-configuration`
- `/jwks.json`
- `/authorize`
- `/token`
- `/userinfo`
- `/login`
- `/consent`
- `/policy/evaluate`

## Token Strategy

- Access token: JWT, signed with `Ed25519` / `EdDSA`
- ID token: OIDC login token, not for API authorization
- Refresh token: hashed at rest, rotated on use, reuse detection enabled
- Authorization code: short-lived, hashed at rest, bound to PKCE and redirect URI

Access-token claims should stay minimal:

- `iss`
- `sub`
- `aud`
- `exp`
- `iat`
- `jti`
- `scope`

Optional small claims:

- `roles`
- `client_id`
- `azp`

## Authorization Model

### RBAC

Coarse-grained authorization through:

- `role`
- `permission`
- `role_permission`
- `subject_role_binding`

### ABAC

Fine-grained runtime decisions through:

- `subject`
- `resource`
- `action`
- `context`

Policies are stored in the database as rules with:

- `effect`
- `resource_pattern`
- `action_pattern`
- `condition_expression`
- `priority`
- `is_active`

Decision model:

- deny overrides allow
- deterministic priority order
- structured decision output with `decision`, `matched_rules`, `reason`

## Data Model

Core tables:

- `users`
- `password_credentials`
- `sessions`
- `oauth_clients`
- `oauth_authorization_codes`
- `oauth_refresh_tokens`
- `oauth_consents`
- `roles`
- `permissions`
- `role_permissions`
- `subject_role_bindings`
- `policy_rules`
- `audit_logs`

## Security Principles

- PostgreSQL from the start
- HTTPS in non-local environments
- Asymmetric signing only
- Argon2id password hashing
- PKCE for public code clients
- Exact redirect URI matching
- Small access tokens
- Audit security-relevant events

