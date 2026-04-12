# Separate Management Listener

## Goal

Allow built-in management endpoints to run on a dedicated listener instead of sharing the proxy listener.

After this phase:

- `server.management.listen` may bind a dedicated management socket
- when `server.management.listen` is unset, management endpoints keep working on the main gateway listener exactly as they do now
- when `server.management.listen` is set, `/__gateway/*` is served only on the management listener
- the management listener serves built-in management endpoints only and does not proxy application traffic
- optional bearer auth still applies to management endpoints in either mode

This phase does not add a separate management metrics registry, admin UI, or dynamic reconfiguration.

## Configuration

```yaml
server:
  listen: 0.0.0.0:8080
  management:
    listen: 127.0.0.1:9000
    auth_token: secret-token
```

`server.management.listen` is optional.

- unset: current shared-listener behavior
- set: dedicated management listener, main listener stops intercepting `/__gateway/*`

## Routing Rules

- shared mode:
  - main listener serves proxy routes and built-in management endpoints
- dedicated mode:
  - main listener serves proxy routes only
  - management listener serves built-in management endpoints only
  - non-management paths on the management listener return `404 Not Found`
  - `/__gateway/*` on the main listener is treated like any other proxy path and therefore returns `404` unless user routes match it

## Architecture

- `config` adds `ManagementConfig.listen`
- `main` optionally binds a second listener and spawns a dedicated accept loop for management traffic
- `route` gains an explicit switch for whether built-in management interception is enabled on a given listener
- `management` gains a connection-serving entry point for management-only traffic so proxy routing is never reached on the management listener
- both listeners share the same in-memory metrics collector so management and proxy traffic remain visible in one snapshot

## Error Handling

- invalid `server.management.listen` fails at startup like the main listener
- management auth failures remain `401 Unauthorized`
- non-management paths on the management listener return `404 Not Found`
- if the dedicated management listener is configured but cannot bind, process startup fails

## Testing

- config parsing for `server.management.listen`
- integration test proving shared-listener behavior remains unchanged by default
- integration test proving dedicated management listener serves `/__gateway/health`
- integration test proving `/__gateway/health` is no longer intercepted on the main listener when a dedicated management listener exists
- integration test proving non-management paths return `404` on the management listener

## Commit Plan

1. docs: add separate management listener spec and plan
2. feat: add dedicated management listener support
