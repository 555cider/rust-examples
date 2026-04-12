# Retry Policy

## Goal

Extend route retry behavior beyond transport failures so routes can retry selected upstream response statuses with a fixed backoff.

After this phase:

- routes may list retryable upstream statuses
- routes may configure a fixed backoff between retry attempts
- existing retry behavior for transport failures and timeouts stays intact
- when retries are exhausted, the last upstream response is returned

This phase does not add exponential backoff, jitter, idempotency detection, or method-specific retry policy.

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
    retry_attempts: 1
    retry_statuses: [503]
    retry_backoff_ms: 25
    path: /
```

If `retry_statuses` is omitted, only transport failures and timeouts are retried, matching current behavior.

## Retry Rules

- transport failures and timeouts remain retryable
- upstream responses are retried only when their status code appears in `retry_statuses`
- `retry_attempts` still means extra attempts after the first request
- the gateway sleeps `retry_backoff_ms` before each retry attempt when the value is greater than zero
- when the last attempt still receives a retryable status, that final response is returned to the client

## Architecture

- `config` adds `retry_statuses` and `retry_backoff_ms`
- `upstream` keeps retry orchestration and decides whether to retry based on either error type or response status
- request body buffering already in place continues to support replaying the same request across attempts

## Testing

- config parsing for `retry_statuses` and `retry_backoff_ms`
- integration test proving a retryable upstream status fails over to the next backend
- regression coverage for transport retry, timeout retry, and normal response passthrough

## Commit Plan

1. docs: add retry policy spec and plan
2. feat: add status-based retry policy
