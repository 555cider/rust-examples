# Upstream Timeout

## Goal

Add route-level upstream request timeouts so the gateway can fail slow backends predictably.

After this phase:

- a route may define `timeout_ms`
- upstream requests that exceed the timeout return `504 Gateway Timeout`
- routes without an explicit timeout keep a sensible default

This phase does not add retries, per-phase connect/read/write timeouts, or circuit breaking.

## Configuration

Each route may define:

```yaml
routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    timeout_ms: 500
    path: /
```

If `timeout_ms` is omitted, the gateway should use a default of `30000`.

## Architecture

- `config` adds `timeout_ms` on routes with a default helper
- `upstream` wraps the request/response future in `tokio::time::timeout`
- `entity` adds a timeout-specific gateway error variant
- `route` maps timeout failures to `504 Gateway Timeout`

## Testing

- config parsing/default test for `timeout_ms`
- integration test for a slow upstream returning `504`
- regression coverage for existing fast routes

## Commit Plan

1. docs: add upstream timeout spec and plan
2. feat: add route-level upstream timeout handling
