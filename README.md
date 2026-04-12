# hyper-gateway

Lightweight Rust API gateway with SCG-style YAML routes, optional registry-backed discovery, retry/timeout/circuit-breaker controls, TLS support, and built-in management endpoints.

## Current Shape

This project is already usable as a static or discovery-backed gateway, but it is not full Spring Cloud Gateway parity yet.

Implemented:

- SCG-style `spring.cloud.gateway.routes`
- static upstream routes and `lb://service` routes
- DNS, Consul, and Eureka-backed discovery
- HTTP/1, H2C, and H2 upstream support
- upstream HTTPS and inbound TLS termination
- retry, timeout, circuit breaker, and in-memory rate limiting
- request/response header filters
- management endpoints: `/__gateway/health`, `/__gateway/routes`, `/__gateway/metrics`

Not implemented yet:

- discovery locator auto-routes
- Kubernetes/Eureka/Consul watch-based providers beyond polling
- end-to-end request-body streaming with retries
- body predicates or body rewrite filters

## Running

Build and run with the default repository config:

```bash
cargo run
```

Pick a different config file with a CLI flag:

```bash
cargo run -- --config examples/consul.yaml
```

Or with an environment variable:

```bash
GATEWAY_CONFIG=config.local.yaml cargo run
```

Config path precedence:

1. `--config <path>` or `-c <path>`
2. positional config path
3. `GATEWAY_CONFIG`
4. `./config.yaml`

## Config Files

- Default runtime file: [config.yaml](/home/my/dev/hyper-gateway/config.yaml:1)
- Static route example: [examples/static.yaml](/home/my/dev/hyper-gateway/examples/static.yaml:1)
- Consul discovery example: [examples/consul.yaml](/home/my/dev/hyper-gateway/examples/consul.yaml:1)
- Eureka discovery example: [examples/eureka.yaml](/home/my/dev/hyper-gateway/examples/eureka.yaml:1)
- Dedicated management example: [examples/management.yaml](/home/my/dev/hyper-gateway/examples/management.yaml:1)
- Expanded configuration notes: [docs/configuration-examples.md](/home/my/dev/hyper-gateway/docs/configuration-examples.md:1)

`config.local.yaml` is a good pattern for machine-local runtime config. It is ignored by git.

## Request Bodies

Routing is based on path, host, method, headers, and query string. The gateway does not inspect request body format for routing.

- `retry_attempts == 0`: request bodies stream to upstream immediately
- `retry_attempts > 0`: request bodies are buffered so retries can replay them
- response bodies stream back to clients

Common request types such as `application/json`, `application/x-www-form-urlencoded`, and `multipart/form-data` are proxied as normal HTTP bodies.

## Management Endpoints

By default management endpoints are served from the main listener:

- `GET /__gateway/health`
- `GET /__gateway/routes`
- `GET /__gateway/metrics`

You can move them to a dedicated listener with:

```yaml
server:
  management:
    listen: 127.0.0.1:9000
    auth_token: change-me
```

## Verification

```bash
cargo test
```
