# Management Metrics

## Goal

Add a built-in management metrics endpoint that exposes in-process request counters for the current gateway instance.

After this phase:

- `GET /__gateway/metrics` returns a JSON snapshot of gateway request metrics
- metrics are tracked per gateway instance, not process-global
- metrics include total requests, per-route status counts, and gateway error counts
- `GET /__gateway/metrics` does not count itself in the snapshot

This phase does not add Prometheus text format, histograms, percentiles, or persistent storage.

## Response Shape

```json
{
  "requests_total": 2,
  "routes": [
    {
      "route": "/",
      "requests": 1,
      "responses_by_status": {
        "200": 1
      },
      "total_duration_ms": 4
    },
    {
      "route": "/__gateway/health",
      "requests": 1,
      "responses_by_status": {
        "200": 1
      },
      "total_duration_ms": 0
    }
  ],
  "errors_by_kind": {
    "Io": 1
  }
}
```

## Recording Rules

- successful upstream responses record against the matched route path
- management endpoints record against their management path
- throttle failures record under `__throttle`
- route-not-found responses record under `__unmatched`
- other gateway-side failures record under `__gateway_error`
- `errors_by_kind` increments only when a `GatewayError` existed
- duration is measured wall-clock from request entry to final gateway response mapping

## Architecture

- add a `metrics` module with an `Arc<Mutex<...>>` collector and serializable snapshot types
- `main` and test helpers create one collector per gateway instance and pass it into `route`
- `route::serve_connection` records metrics after management handling, throttle handling, route resolution, and upstream completion
- `management` reads the current snapshot from the collector when serving `/__gateway/metrics`

## Testing

- integration test proving the metrics snapshot includes management and proxied requests
- integration test proving gateway-side upstream failures appear in `errors_by_kind`
- regression coverage for existing routes, management endpoints, and proxy behavior

## Commit Plan

1. docs: add management metrics spec and plan
2. feat: add built-in management metrics endpoint
