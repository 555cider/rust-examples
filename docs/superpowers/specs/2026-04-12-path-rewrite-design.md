# Path Rewrite

## Goal

Add a simple prefix rewrite filter so matched requests can be forwarded to a different upstream path.

After this phase:

- a route may define optional `rewrite_prefix`
- route matching still uses `path`
- when `rewrite_prefix` is set, the matched route prefix is replaced before forwarding upstream

This phase does not add regex rewrite, redirect filters, or response header filters.

## Configuration

```yaml
routes:
  - scheme: http
    protocol: http1
    authority:
      host: 127.0.0.1
      port: 8001
    path: /api
    rewrite_prefix: /
```

Examples:

- incoming `/api/users?id=7` -> upstream `/users?id=7`
- incoming `/api` -> upstream `/`
- `rewrite_prefix: /v1` with incoming `/api/users` -> upstream `/v1/users`

If `rewrite_prefix` is omitted, behavior remains unchanged.

## Matching And Rewrite Rules

- route selection still uses the original request path
- rewrite happens only after the route is selected
- the route's `path` prefix is replaced exactly once
- query string is preserved
- `rewrite_prefix` must start with `/`

## Architecture

- `config` adds optional `rewrite_prefix`
- `route` computes the forwarded path-and-query after matching and before the request template is stored
- retry and load balancing reuse the rewritten path because they already rebuild requests from the stored template

## Error Handling

- invalid `rewrite_prefix` values return `502 Bad Gateway` through existing invalid-route-config handling
- unmatched routes still return `404 Not Found`

## Testing

- config parsing for `rewrite_prefix`
- unit tests for prefix replacement and query preservation
- integration test proving rewritten upstream path is observed by the backend
- regression coverage for routes without rewrite

## Commit Plan

1. docs: add path rewrite spec and plan
2. feat: add route prefix rewrite support
