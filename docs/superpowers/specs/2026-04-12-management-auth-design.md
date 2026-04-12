# Management Auth

## Goal

Add optional bearer-token protection for built-in management endpoints.

After this phase:

- `server.management.auth_token` may protect all `/__gateway/*` endpoints
- when configured, management requests without a matching `Authorization: Bearer <token>` header return `401 Unauthorized`
- successful management requests still bypass throttle and upstream routing
- when not configured, current management endpoint behavior stays unchanged

This phase does not add per-endpoint policy, multiple tokens, OAuth, or proxy route authentication.

## Configuration

```yaml
server:
  management:
    auth_token: secret-token
```

If `server.management.auth_token` is omitted, management endpoints remain publicly reachable on the gateway listener.

## Auth Rules

- the token applies to all built-in management endpoints:
  - `/__gateway/health`
  - `/__gateway/routes`
  - `/__gateway/metrics`
- the gateway expects the exact header form `Authorization: Bearer <token>`
- missing header, wrong scheme, or wrong token returns `401 Unauthorized`
- unauthorized responses include `WWW-Authenticate: Bearer`

## Architecture

- `config` adds `ServerConfig.management.auth_token`
- `route` passes management config through to the management handler
- `management` performs auth before serving any management response body
- no proxy-route auth behavior changes in this phase

## Testing

- config parsing for `server.management.auth_token`
- integration test proving management access is denied without the token
- integration test proving management access succeeds with the configured token
- regression coverage for existing management endpoints and proxy behavior

## Commit Plan

1. docs: add management auth spec and plan
2. feat: add management bearer token auth
