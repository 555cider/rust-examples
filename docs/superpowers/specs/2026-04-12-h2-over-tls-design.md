# HTTP/2 Over TLS

## Goal

Phase 3 extends the gateway transport layer with HTTP/2 over TLS support.

After this phase, the gateway should support:

- `protocol: h2` for HTTPS upstream routes
- HTTPS clients speaking HTTP/2 to the gateway through ALPN
- continued support for existing `http1` and `h2c` routes

This phase does not add cleartext prior-knowledge upgrades for clients, gRPC-specific behavior, or HTTP/2-specific policy controls.

## Configuration

The route protocol enum expands to:

- `http1`
- `h2c`
- `h2`

`h2` means HTTP/2 over TLS. It is only valid with `scheme: https`.

## Architecture

- `config` adds the new `h2` protocol value.
- `tls` becomes the central place for ALPN-aware client and server TLS configuration.
- `connector` builds a TLS client config with ALPN based on the selected route protocol.
- `upstream` maps `protocol: h2` to the existing Hyper HTTP/2 client handshake over a TLS stream.
- inbound TLS termination advertises both `h2` and `http/1.1` so Hyper server auto-detection can serve either protocol on the same listener.

## Error Handling

- invalid `scheme` / `protocol` combinations should fail fast as config errors
- ALPN or TLS handshake failures should continue mapping to `502 Bad Gateway` on upstream paths
- inbound TLS handshake failures remain connection-level errors with logs

## Testing

- config parsing for `protocol: h2`
- integration test for `HTTP/1 client -> gateway -> HTTPS h2 upstream`
- integration test for `HTTPS h2 client -> gateway -> HTTP upstream`
- regression coverage for existing HTTP/1, H2C, and HTTPS HTTP/1 paths

## Commit Plan

1. docs: add phase-3 h2-over-tls spec and plan
2. feat: add h2 over tls transport support
