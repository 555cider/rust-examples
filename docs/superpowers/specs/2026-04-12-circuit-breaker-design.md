# Circuit Breaker

## Goal

Add a minimal per-route circuit breaker that stops repeatedly sending requests to a failing upstream.

After this phase:

- a route may optionally define a circuit breaker
- consecutive retry-exhausted transport failures increment the breaker failure count
- once the failure threshold is reached, the breaker opens and new requests fail fast with `503 Service Unavailable`
- after the open cooldown elapses, the next request is allowed through
- a successful request resets the breaker back to closed

This phase does not add half-open concurrency limits, rolling windows, per-upstream breaker state, or breaker metrics.

## Configuration

```yaml
routes:
  - scheme: http
    protocol: http1
    authority:
      host: 127.0.0.1
      port: 8001
    circuit_breaker:
      failure_threshold: 2
      open_ms: 250
    path: /
```

If `circuit_breaker` is omitted, the route keeps current behavior.

## Breaker Rules

- the breaker state is keyed per route path
- only retry-exhausted transport failures count:
  - connect errors
  - upstream handshake/send errors
  - route timeout errors
- route-not-found and invalid-config errors do not affect breaker state
- when the breaker opens, the current failing request still returns its original upstream error status (`502` or `504`)
- while open, new requests short-circuit before any upstream connection attempt and return `503`
- once `open_ms` has elapsed, the next request is allowed through
- a successful upstream response closes the breaker and clears the consecutive failure count

## Architecture

- `config` adds an optional `CircuitBreakerConfig`
- `upstream` owns breaker state because it already owns retry exhaustion, timeout handling, and success/failure outcomes
- breaker state lives in a process-global map keyed by route path, protected by a mutex
- `entity` adds an explicit `CircuitOpen` error so `route` can map it to `503`

## Testing

- config parsing for the breaker block
- unit test for `CircuitOpen` status mapping
- integration test proving:
  - the first failing request still returns `502`
  - the next immediate request returns `503`
  - a request after cooldown reaches upstream again and succeeds
- regression coverage for existing retry, timeout, and routing behavior

## Commit Plan

1. docs: add circuit breaker spec and plan
2. feat: add route-level circuit breaker
