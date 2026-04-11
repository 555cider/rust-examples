# Network Foundation Redesign

## Goal

Phase 1 turns `hyper-gateway` from a fixed local proxy into a configurable network edge that can:

- bind from configuration instead of a hard-coded `127.0.0.1:8080`
- route to upstreams addressed by hostname or IP
- connect to upstreams over plain HTTP or HTTPS/TLS

This phase does not add client-to-gateway TLS termination, retries, timeouts, circuit breakers, discovery, or load balancing.

## Current Problems

- bootstrap is hard-coded in `src/main.rs`
- upstream connection logic requires literal IP addresses
- `scheme` only changes the rewritten URI string and does not affect transport
- routing, request rewriting, address resolution, and transport setup are too tightly coupled

## Target Architecture

### Bootstrap

`src/main.rs` should only:

- initialize logging
- load config
- derive the listen target from config
- initialize throttling
- accept sockets and delegate each connection to `route::run`

### Configuration

`src/config.rs` should own typed configuration for:

- `server.listen`
- `routes[]`

The config layer should validate enough structure to fail fast on malformed files. Route matching behavior remains longest path-prefix match.

### Routing

`src/route.rs` should own:

- selecting the route for a request
- rewriting the upstream URI
- rewriting `Host`
- preserving or regenerating `traceparent`

It should not know how sockets are opened.

### Connector Layer

A new `src/connector.rs` should own:

- hostname and IP resolution
- deciding between plain TCP and TLS based on route scheme
- returning an I/O stream suitable for the selected upstream protocol

This layer is the foundation for later timeout, retry, and load-balancing work.

### Upstream Dispatch

`src/upstream.rs` should become a thin orchestration layer that:

- receives a routed request plus route transport metadata
- asks `connector` for the correct upstream transport
- performs the Hyper client handshake for the selected protocol
- forwards the request and wraps the response body

## Config Shape

The target config shape for this phase is:

```yaml
server:
  listen: 127.0.0.1:8080

routes:
  - scheme: http
    protocol: http1
    authority:
      host: localhost
      port: 8001
    path: /
```

To avoid needless breakage during the redesign, the parser may accept the legacy top-level `route:` key as an alias for `routes:`.

## Error Handling

The gateway should distinguish these internal failures:

- invalid config
- route not found
- DNS resolution failure
- TCP connect failure
- TLS handshake failure
- upstream protocol failure

External behavior for transport failures in this phase should normalize to `502 Bad Gateway`. Route misses stay `404 Not Found`. Validation failures remain startup errors.

## Testing Strategy

### Unit Tests

- config parsing for `server.listen`
- config parsing for hostname authorities
- route selection remains longest-prefix
- connector rejects unsupported or malformed transport inputs

### Integration Tests

- HTTP/1 gateway to HTTP/1 upstream over IP
- HTTP/1 gateway to HTTP/1 upstream over hostname
- HTTP/1 gateway to HTTPS upstream
- mixed route selection still picks the most specific path
- trace header behavior still works across the redesigned transport path

## Commit Plan

Phase 1 should land in multiple commits:

1. docs: add network foundation design and implementation plan
2. refactor: redesign config and bootstrap for network foundation
3. feat: add connector abstraction and hostname resolution
4. feat: add upstream https transport support

## Non-Goals

- TLS termination for inbound client traffic
- HTTP/2 over TLS (`h2`) support
- dynamic discovery or load balancing
- retries, timeouts, circuit breakers
- auth, metrics, and actuator-style endpoints
