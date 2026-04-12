# Configuration Examples

The gateway currently reads `config.yaml` from its process working directory at startup. The binary does not take a separate config path yet, so the practical place to put your runtime config is the repository root `config.yaml`.

## Static Routes

```yaml
server:
  listen: 127.0.0.1:8080

spring:
  cloud:
    gateway:
      routes:
        - id: users
          uri: http://127.0.0.1:8001
          predicates:
            - Path=/users/**

        - id: products
          uri: http://127.0.0.1:8002
          predicates:
            - Path=/products/**
          filters:
            - StripPrefix=1
```

## Consul-Backed Discovery

```yaml
server:
  listen: 127.0.0.1:8080
  discovery:
    refresh_ms: 5000
    consul:
      address: http://127.0.0.1:8500
      token: your-consul-token
      passing_only: true

spring:
  cloud:
    gateway:
      routes:
        - id: products
          uri: lb://products
          predicates:
            - Path=/products/**
          metadata:
            timeoutMs: 3000
            retryAttempts: 1
            retryStatuses: 502,504
            retryBackoffMs: 100
```

## Eureka-Backed Discovery

```yaml
server:
  listen: 127.0.0.1:8080
  discovery:
    refresh_ms: 5000
    eureka:
      service_url: http://127.0.0.1:8761/eureka/v2/apps
      username: admin
      password: secret
      prefer_ip_address: true

spring:
  cloud:
    gateway:
      routes:
        - id: inventory
          uri: lb://inventory
          predicates:
            - Path=/inventory/**
          metadata:
            scheme: https
            protocol: h2
            timeoutMs: 5000
```

## Management Endpoints

```yaml
server:
  listen: 127.0.0.1:8080
  management:
    listen: 127.0.0.1:9000
    auth_token: change-me
```

With that config:

- proxy traffic uses `127.0.0.1:8080`
- `GET /__gateway/health`, `GET /__gateway/routes`, and `GET /__gateway/metrics` move to `127.0.0.1:9000`
- management requests must send `Authorization: Bearer change-me`

## Request Body Handling

The gateway routes requests by path, host, method, headers, and query string. It does not inspect body format before proxying.

These request types are forwarded as ordinary HTTP bodies:

- `application/json`
- `application/x-www-form-urlencoded`
- `multipart/form-data`
- other content types that fit in a normal HTTP request body

Examples:

```bash
curl -X POST http://127.0.0.1:8080/users \
  -H 'Content-Type: application/json' \
  -d '{"name":"alice","role":"admin"}'
```

```bash
curl -X POST http://127.0.0.1:8080/users \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data 'name=alice&role=admin'
```

```bash
curl -X POST http://127.0.0.1:8080/users \
  -F 'name=alice' \
  -F 'file=@./avatar.png'
```

Current caveats:

- request bodies are fully buffered in memory before retry/proxy, not streamed end-to-end
- header filters can still break uploads if they remove or overwrite `Content-Type`
- there are no body-based predicates or body-rewrite filters yet
