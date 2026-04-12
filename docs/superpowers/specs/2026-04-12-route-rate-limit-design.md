# Route Rate Limit

## Goal

Replace the current hard-coded SQLite throttle with route-level in-memory rate limiting.

After this phase:

- routes may optionally define a local fixed-window rate limit
- unlimited routes no longer require an `api-key` header
- limited routes require the configured key header and return `400` when it is missing
- requests over the configured limit return `429 Too Many Requests`

This phase does not add distributed rate limiting, persistent storage, Redis, or sliding-window algorithms.

## Configuration

```yaml
routes:
  - scheme: http
    protocol: http1
    authority:
      host: 127.0.0.1
      port: 8001
    rate_limit:
      requests: 2
      window_ms: 1000
      key_header: api-key
    path: /
```

`key_header` defaults to `api-key` when omitted.

## Rate Limit Rules

- rate limiting is keyed by `(route path, key header value)`
- the limiter uses a fixed window with in-memory counters per gateway instance
- when the key header is missing or invalid UTF-8, the gateway returns `400 Bad Request`
- when the request count exceeds `requests` inside the current window, the gateway returns `429 Too Many Requests`
- when `rate_limit` is omitted, the route is not limited and no key header is required

## Architecture

- `config` adds optional `RateLimitConfig` to each route
- `throttle` becomes an in-memory limiter store instead of the current SQLite-backed schema
- `route` resolves the route first, runs the limiter against the matched route, then prepares the proxied request body
- management endpoints still bypass rate limiting

## Testing

- config parsing for route rate-limit config
- integration test proving unlimited routes work without an API key
- integration test proving limited routes return `400` without the key header
- integration test proving limited routes return `429` after the configured limit is exceeded
- regression coverage for existing proxy, management, retry, and TLS behavior

## Commit Plan

1. docs: add route rate limit spec and plan
2. feat: add route-level in-memory rate limiting
