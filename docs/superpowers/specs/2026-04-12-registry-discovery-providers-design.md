# Registry-Backed Discovery Providers

## Goal

Make `lb://service` routes use a real service registry instead of relying only on DNS host resolution.

After this phase:

- the gateway accepts optional global discovery provider config under `server.discovery`
- `lb://service` routes may resolve service instances from Consul or Eureka
- the gateway polls the configured registry on a refresh interval and keeps an in-memory snapshot
- request routing uses the latest snapshot to choose a discovered instance round-robin
- existing static routes and DNS-backed behavior keep working when no registry provider is configured

This phase does not add the Spring Cloud Gateway discovery locator auto-route feature, Kubernetes API watches, Zookeeper, or weighted load balancing.

## Scope Choice

There are three reasonable cuts for discovery support:

1. Registry-backed resolution for explicit `lb://service` routes only
2. Registry-backed resolution plus SCG discovery locator auto-generated routes
3. Full DiscoveryClient parity with include expressions, route-id prefixes, and custom locator predicates/filters

This phase implements option 1.

It gives immediate production value without forcing a larger rewrite of route matching. Once explicit `lb://service` routes work against registries, the auto-route locator can be layered on top of the same discovery cache in a later phase.

## Configuration

### Consul

```yaml
server:
  discovery:
    refresh_ms: 5000
    consul:
      address: http://127.0.0.1:8500
      token: ${CONSUL_HTTP_TOKEN}
      passing_only: true

spring:
  cloud:
    gateway:
      routes:
        - id: products
          uri: lb://products
          predicates:
            - Path=/products/**
```

### Eureka

```yaml
server:
  discovery:
    refresh_ms: 5000
    eureka:
      service_url: http://127.0.0.1:8761/eureka/v2/apps
      prefer_ip_address: true

spring:
  cloud:
    gateway:
      routes:
        - id: inventory
          uri: lb://inventory
          predicates:
            - Path=/inventory/**
```

Rules:

- at most one provider may be configured at a time
- `refresh_ms` defaults to a small polling interval appropriate for local cache refresh
- when Consul ACLs are enabled, the gateway passes `X-Consul-Token`
- legacy static routes do not require any discovery config

## Route Behavior

- `lb://service` means the backend host/port comes from the discovery snapshot
- `lb://service:8443` keeps the discovered host but overrides the discovered port with `8443`
- SCG metadata `port` still acts as a port override for `lb://service`
- retries advance to the next discovered instance because each connect attempt selects the next instance in the snapshot
- if no discovery provider is configured, existing DNS-backed `lb://` fallback behavior remains available

## Provider Semantics

### Consul

- poll `GET /v1/health/service/<service>?passing` when `passing_only` is enabled
- use `Service.Address` when present, otherwise fall back to `Node.Address`
- use the registered service port unless the route explicitly overrides it

### Eureka

- poll `GET /eureka/v2/apps/<APP>` with `Accept: application/json`
- filter instances to `status=UP`
- choose host from `ipAddr` when `prefer_ip_address=true`, otherwise use `hostName` with fallback to `ipAddr`
- use the registered `securePort` for `https` routes when it is enabled, otherwise use the normal `port`
- route-level port overrides still win over registry-reported ports

## Startup And Refresh Behavior

- when a registry provider is configured and at least one discovery route exists, the gateway performs an initial fetch before serving traffic
- if the initial fetch fails, startup fails fast
- later refresh failures do not clear the last good snapshot; the gateway keeps serving from cached instances and logs refresh errors
- if a discovered service currently has no instances, requests for that service return `502 Bad Gateway`

## Architecture

- `config` adds `ServerConfig.discovery` and route-level discovery metadata in the internal route model
- a new `discovery` module owns provider clients, polling, snapshot storage, and service-instance normalization
- `main` initializes the discovery registry once and shares it with gateway request handlers
- `connector` asks the discovery registry for the next authority when a route is discovery-backed, then resolves that authority to socket addresses as usual
- `upstream` keeps its retry loop; retries naturally rotate across discovered instances because connect selection is per attempt

## Testing

- config parsing for Consul and Eureka discovery settings
- config parsing for discovery-backed `lb://` routes with and without port overrides
- unit coverage for Consul response parsing
- unit coverage for Eureka response parsing
- integration test proving a Consul-backed `lb://service` route proxies traffic
- integration test proving a Consul-backed discovery refresh picks up instance changes
- integration test proving a Eureka-backed `lb://service` route proxies traffic
- regression coverage for DNS-backed `lb://` fallback and existing static routes

## Commit Plan

1. docs: add registry discovery providers spec and plan
2. feat: add registry-backed discovery providers
