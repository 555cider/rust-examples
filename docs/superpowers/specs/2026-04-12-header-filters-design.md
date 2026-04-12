# Header Filters

## Goal

Add simple per-route request and response header filters.

After this phase:

- a route may statically remove request headers
- a route may statically set request headers
- a route may statically remove response headers
- a route may statically set response headers

This phase does not add regex-based rewrites, conditional filters, or body transforms.

## Configuration

```yaml
routes:
  - scheme: http
    protocol: http1
    authority:
      host: 127.0.0.1
      port: 8001
    request_header_filter:
      remove: [x-remove-me]
      set:
        x-added-by-gateway: enabled
    response_header_filter:
      remove: [server]
      set:
        x-gateway: hyper-gateway
    path: /
```

If either filter block is omitted, that side is left unchanged.

## Filter Rules

- request filter order: remove headers, set headers, then gateway-managed `traceparent` and `Host` are applied
- response filter order: remove headers, then set headers
- header names are matched case-insensitively through normal HTTP header parsing
- invalid configured header names or values are treated as invalid route config

## Architecture

- `config` adds reusable `HeaderFilterConfig`
- `route` applies request header filters while preparing the reusable request template
- `upstream` applies response header filters after receiving the upstream response and before returning it to the client
- retry and load balancing reuse the already-filtered request template

## Testing

- config parsing for request and response filters
- unit test for remove-then-set behavior
- integration test proving request headers are rewritten before reaching upstream
- integration test proving response headers are rewritten before reaching the client
- regression coverage for existing routing and transport behavior

## Commit Plan

1. docs: add header filter spec and plan
2. feat: add request and response header filters
