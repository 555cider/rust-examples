# Route Predicates

## Goal

Extend route matching beyond path prefix so routes may also require specific hosts and HTTP methods.

After this phase:

- a route may define optional `hosts`
- a route may define optional `methods`
- matching still prefers the longest path prefix among routes whose predicates all match

This phase does not add header, query, cookie, or weight predicates.

## Configuration

```yaml
routes:
  - scheme: http
    protocol: http1
    authority:
      host: 127.0.0.1
      port: 8001
    hosts: [api.example.com]
    methods: [GET, HEAD]
    path: /api
```

If `hosts` is omitted, any host matches. If `methods` is omitted, any method matches.

## Matching Rules

- path: request path must start with the route's `path`
- host: compare against the request `Host` header when present, otherwise request URI authority
- host comparison ignores port and is case-insensitive
- method: compare exact HTTP method names after normalizing configured values to uppercase
- winner selection remains longest matching path prefix among routes whose predicates all match

## Architecture

- `config` adds optional `hosts` and `methods`
- `route` owns predicate evaluation so matching can see request path, host, and method together
- existing path-prefix helper behavior remains the fallback when no extra predicates are configured

## Error Handling

- unmatched predicate combinations continue to return `404 Not Found`
- malformed host headers do not crash matching; they simply fail host-specific routes

## Testing

- config parsing for `hosts` and `methods`
- unit tests for host normalization and predicate matching
- integration test proving host-specific route selection
- integration test proving method-specific route selection
- regression coverage for existing path-based route selection

## Commit Plan

1. docs: add route predicate spec and plan
2. feat: add host and method route predicates
