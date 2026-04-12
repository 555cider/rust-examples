# Management Endpoints

## Goal

Add minimal built-in management endpoints for basic operational visibility without depending on any upstream service.

After this phase:

- `GET /__gateway/health` returns a local gateway health response
- `GET /__gateway/routes` returns the currently loaded route configuration as JSON
- management endpoints are handled inside the gateway and never forwarded upstream
- management endpoints bypass route matching and throttle checks

This phase does not add authentication, metrics, live dependency checks, or a separate management listener.

## Endpoints

### `GET /__gateway/health`

- response status: `200 OK`
- response body: `{"status":"UP"}`
- response content type: `application/json`

### `GET /__gateway/routes`

- response status: `200 OK`
- response body: JSON object with a top-level `routes` array
- each route entry is the current serialized route config
- route order matches the loaded configuration order

### Unsupported Methods

- non-`GET` requests to either management path return `405 Method Not Allowed`

## Architecture

- add a small `management` module that inspects incoming requests before throttle and upstream routing
- keep management response building local to the gateway, using a direct bytes body variant in `GatewayBody`
- `route::serve_connection` checks management endpoints first, then falls back to throttle and normal routing

## Testing

- integration test proving `/__gateway/health` responds without any configured upstream route or API key
- integration test proving `/__gateway/routes` returns the configured route list without touching upstream
- regression coverage for existing routing, throttle, and upstream behavior

## Commit Plan

1. docs: add management endpoint spec and plan
2. feat: add built-in management endpoints
