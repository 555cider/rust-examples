# Inbound TLS Termination

## Goal

Phase 2 adds optional TLS termination for client connections to the gateway.

After this phase, the gateway should be able to:

- accept plain HTTP traffic as it does today
- accept HTTPS client traffic when `server.tls` is configured
- terminate TLS at the gateway and continue routing to existing HTTP, H2C, or HTTPS upstreams

This phase does not add mutual TLS, ALPN-based HTTP/2 over TLS, automatic certificate reloads, or ACME automation.

## Target Shape

The server config expands to:

```yaml
server:
  listen: 127.0.0.1:8443
  tls:
    cert_path: /path/to/server-cert.der
    key_path: /path/to/server-key.der
```

If `server.tls` is absent, the gateway continues to accept plain TCP. If it is present, the gateway must complete a TLS handshake before handing the connection to Hyper.

## Architecture

- `config` gains a typed `server.tls` block.
- `route` exposes a generic connection-serving function that can run over either plain TCP or a TLS stream.
- `main` owns the accept loop and decides whether each accepted socket is passed directly to `route` or wrapped by a `TlsAcceptor` first.
- test helpers mirror the production path so HTTPS client integration tests exercise the same generic route-serving code.

## Error Handling

- malformed TLS config remains a startup/config error
- TLS handshake failures should be logged and the connection dropped
- request processing after successful handshake follows the existing `404` and `502` behavior

## Testing

- config parsing for `server.tls`
- integration test for `HTTPS client -> gateway TLS termination -> HTTP upstream`
- regression tests for existing plain HTTP traffic

## Commit Plan

1. docs: add inbound TLS termination design and plan
2. feat: add optional gateway TLS termination
