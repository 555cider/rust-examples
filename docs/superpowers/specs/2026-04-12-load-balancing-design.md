# Static Round-Robin Load Balancing

## Goal

Add simple per-route load balancing across a static set of configured upstreams.

After this phase:

- a route may keep its existing primary `authority`
- a route may also define extra `upstreams`
- requests to that route are distributed round-robin across the full upstream pool

This phase does not add active health checks, weighted balancing, sticky sessions, or discovery-backed backends.

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
      - host: 127.0.0.1
        port: 8003
    path: /
```

The primary `authority` remains the first backend. `upstreams` appends additional backends. If `upstreams` is omitted, behavior is unchanged.

## Architecture

- `config` adds optional `upstreams`
- `route` selects one backend authority per request using round-robin state shared by route path
- request rewriting uses the selected authority so `Host` and URI match the chosen backend
- `upstream` connects to the selected authority instead of the route's primary authority

## Error Handling

- if all configured backends are syntactically invalid, the request still fails as a bad gateway
- this phase does not try another backend after a connection failure; balancing only chooses the initial target

## Testing

- config parsing for `upstreams`
- integration test proving round-robin alternates between two upstreams
- regression coverage for existing single-upstream routes

## Commit Plan

1. docs: add load-balancing spec and plan
2. feat: add static round-robin load balancing
