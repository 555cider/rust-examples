# Retry Failover

## Goal

Add bounded per-route retries for transport failures so a request can fail over to the next configured upstream.

After this phase:

- a route may define `retry_attempts`
- the gateway retries on connect, handshake, protocol, and timeout failures
- each retry advances to the next upstream in the route's round-robin order

This phase does not retry based on upstream HTTP status codes, add backoff, or add active health checks.

## Configuration

```yaml
routes:
  - scheme: http
    protocol: http1
    authority:
      host: 127.0.0.1
      port: 8001
    upstreams:
      - host: 127.0.0.1
        port: 8002
    timeout_ms: 100
    retry_attempts: 1
    path: /
```

`retry_attempts` is the number of extra transport attempts after the initial one. If omitted, behavior remains single-attempt.

## Architecture

- `config` adds `retry_attempts` with default `0`
- `route` stops building a one-shot upstream request and instead prepares a reusable request template
- request bodies are buffered once per incoming request so retries can rebuild the upstream request
- `upstream` owns attempt sequencing:
  - choose the round-robin start index once per request
  - try the selected backend
  - on retryable transport failure, rebuild the request for the next backend

## Retry Policy

- retryable: `Io`, `Hyper`, `Timeout`
- non-retryable: invalid route config, request build errors, upstream HTTP responses
- retries are bounded by `1 + retry_attempts`
- retries do not change route matching or request path rewriting rules

## Tradeoffs

- buffering request bodies enables retries but removes streaming pass-through for request bodies in this phase
- this is acceptable for now because it unlocks correct retry behavior with minimal connector churn

## Testing

- config parsing for `retry_attempts`
- integration test proving a failed primary upstream falls through to the next configured backend
- integration test proving an upstream timeout can fail over when retries are enabled
- regression coverage for existing single-attempt behavior

## Commit Plan

1. docs: add retry failover spec and plan
2. feat: add route-level retry failover
