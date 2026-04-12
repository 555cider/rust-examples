# Header And Query Predicates

## Goal

Extend route matching with exact-match header and query predicates.

After this phase:

- a route may define optional `headers`
- a route may define optional `queries`
- all configured predicates on a route must match before the route is eligible

This phase does not add regex matching, repeated-query semantics, or header transforms.

## Configuration

```yaml
routes:
  - scheme: http
    protocol: http1
    authority:
      host: 127.0.0.1
      port: 8001
    headers:
      x-tenant: acme
    queries:
      region: kr
    path: /search
```

If `headers` or `queries` is omitted, that predicate class is not applied.

## Matching Rules

- header predicate: request must contain the header with the exact configured value
- query predicate: request query string must contain the key with the exact configured value
- route selection still prefers the longest matching path; ties prefer the route with more predicate classes configured
- existing host and method predicate behavior is unchanged

## Architecture

- `config` adds optional `headers` and `queries` maps
- `route` evaluates these predicates alongside path, host, and method
- request forwarding and retry logic remain unchanged because this phase only affects route selection

## Error Handling

- unmatched predicate sets still return `404 Not Found`
- invalid configured header names are treated as route-config errors and surface as `502 Bad Gateway`

## Testing

- config parsing for `headers` and `queries`
- unit tests for exact header and query predicate matching
- integration test proving header-based route selection
- integration test proving query-based route selection
- regression coverage for existing path/host/method routing

## Commit Plan

1. docs: add header and query predicate spec and plan
2. feat: add header and query route predicates
