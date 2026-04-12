# SCG-Style YAML And DNS Discovery

## Goal

Make the gateway usable with a Spring Cloud Gateway-like route definition format while adding real discovery-backed routing for hostname-based services.

After this phase:

- the gateway still accepts the current custom `routes:` format
- the gateway also accepts `spring.cloud.gateway.routes`
- SCG-style routes may use `id`, `uri`, `predicates`, `filters`, and `metadata`
- `http://`, `https://`, and `h2c://` URIs are supported in SCG-style config
- `lb://service[:port]` URIs are supported for DNS-backed discovery
- hostname upstreams are resolved to all DNS A/AAAA records and selected round-robin per connection attempt

This phase does not add Eureka, Consul, Kubernetes API watchers, wildcard host predicates, regex path predicates, or full Spring Cloud Gateway filter parity.

## Why This Phase First

The current gateway is still built around a custom static route model. That makes migration and day-2 operations awkward for users who already know Spring Cloud Gateway or who expect discovery to drive backend selection.

The most practical first correction is:

- accept a familiar route YAML shape
- let service names resolve through DNS each time the gateway connects
- keep the internal runtime route model stable so existing features continue to work

## Supported SCG-Style Route Shape

```yaml
server:
  listen: 127.0.0.1:8080
spring:
  cloud:
    gateway:
      routes:
        - id: products
          uri: lb://products.default.svc.cluster.local:8080
          predicates:
            - Path=/products/**
            - Method=GET
          filters:
            - StripPrefix=1
            - SetRequestHeader=X-Gateway,hyper-gateway
          metadata:
            timeoutMs: 250
            retryAttempts: 1
            retryStatuses: [503, 504]
```

The SCG-style parser maps supported fields into the existing internal route model.

## Supported Predicate Subset

- `Path=/prefix/**`
- `Path=/exact`
- `Host=api.example.com`
- `Host=api.example.com,www.example.com`
- `Method=GET`
- `Method=GET,POST`
- `Header=X-Tenant,acme`
- `Query=region,kr`

Rules:

- `Path` defaults to `/` if omitted
- multiple `Path` entries are rejected in this phase
- wildcard hosts such as `**.example.com` are rejected in this phase
- unsupported predicates fail config loading instead of being silently ignored

## Supported Filter Subset

- `StripPrefix=<n>`
- `PrefixPath=/internal`
- `SetRequestHeader=Name,Value`
- `RemoveRequestHeader=Name`
- `SetResponseHeader=Name,Value`
- `RemoveResponseHeader=Name`

Rules:

- `StripPrefix` and `PrefixPath` are compiled into the existing prefix-rewrite behavior
- unsupported filters fail config loading

## URI Rules

- `http://host:port` -> `scheme=http`, `protocol=http1`
- `https://host:port` -> `scheme=https`, `protocol=http1`
- `h2c://host:port` -> `scheme=http`, `protocol=h2c`
- `lb://service:port` -> DNS-backed hostname discovery, defaulting to `scheme=http`, `protocol=http1`
- `lb://service` may also use `metadata.port`
- `lb://` routes may override `scheme` and `protocol` through `metadata.scheme` and `metadata.protocol`

Examples:

```yaml
uri: h2c://inventory.internal:9000
```

```yaml
uri: lb://orders.default.svc.cluster.local
metadata:
  port: 8443
  scheme: https
  protocol: h2
```

## Discovery Behavior

- hostname authorities are resolved with DNS on each connection attempt
- all resolved socket addresses participate in round-robin selection
- retry attempts against the same authority advance to the next resolved address
- static `upstreams` still work and keep existing per-route round-robin behavior
- DNS lookup failure still returns `502 Bad Gateway`

This gives real discovery-backed routing for DNS-based environments such as Kubernetes DNS and Consul DNS, even though registry-specific APIs are not added yet.

## Metadata Mapping

The SCG-style `metadata` object maps to existing internal route options:

- `protocol`
- `scheme`
- `port`
- `timeoutMs`
- `retryAttempts`
- `retryStatuses`
- `retryBackoffMs`

Unsupported metadata keys are ignored in this phase unless they are required to complete URI parsing.

## Architecture

- `config` keeps the internal runtime route model
- new config transformation code parses either legacy routes or SCG-style route definitions into that model
- `connector` resolves hostname authorities to concrete socket addresses and performs per-authority round-robin
- `upstream` continues to retry across attempts, naturally benefiting from DNS address rotation
- `config.yaml` is updated to show the SCG-style route shape

## Testing

- config parsing for `spring.cloud.gateway.routes`
- config parsing for `lb://` URIs and metadata overrides
- config rejection for unsupported predicates or filters
- unit coverage for DNS address round-robin selection
- integration test proving a SCG-style route proxies traffic
- integration test proving SCG-style `StripPrefix` rewrites the forwarded path
- integration test proving SCG-style `lb://localhost:<port>` routes connect successfully
- regression coverage for existing legacy config and proxy behavior

## Commit Plan

1. docs: add scg yaml and dns discovery spec and plan
2. feat: add scg-style route config and dns-backed discovery
