# CLAUDE.md -- netray-common

## Project Overview

**netray-common** is a shared utility crate for the [netray.info](https://netray.info) service ecosystem. It provides cross-cutting concerns (IP extraction, error formatting, rate limiting, security headers) used by multiple backend services.

- **Author**: Lukas Pustina | **License**: MIT
- **MSRV**: 1.88 (edition 2024)

## CI/CD

Workflow rules: [`specs/rules/workflow-rules.md`](../../specs/rules/workflow-rules.md). Follow those rules when creating or modifying any `.github/workflows/*.yml` file.

Workflows: `ci.yml` (PR gate: fmt, clippy, test, audit), inert until the monorepo CI spec replaces it. Not published: a workspace member that the service crates depend on by path, versioned with the workspace.

## Build & Test

The verbs live in the root `justfile` (see the root `README.md`); run them from the repository root.

```sh
cargo test -p netray-common  # this crate's tests
just adlc-verify             # the gate: fmt-check, clippy, every test, offline
```

## Architecture

```
netray-common/
  Cargo.toml
  src/
    lib.rs                   # crate root, re-exports modules
    config.rs                # the one config loader: optional TOML + env, strict
    cors.rs                  # public-API CORS layer (production parity)
    ip_extract.rs            # real client IP extraction from proxy headers
    error.rs                 # structured JSON error responses (ApiError trait)
    rate_limit.rs            # keyed + global rate limiting (governor wrappers)
    security_headers.rs      # axum middleware for CSP, HSTS, X-Frame-Options, etc.
    telemetry.rs             # tracing-subscriber init + optional OTel OTLP export
```

### Modules

| Module | Purpose |
|--------|---------|
| `config` | `load::<T>(path, prefix)` builds a service config from an optional TOML file and the environment (`<PREFIX><SECTION>__<KEY>`; `<NAME>_CONFIG` names the file and is never a key). The only loader any subcommand uses; every config struct, the shared ones here included, carries `deny_unknown_fields`, so an unknown key in file or env fails the load. |
| `cors` | `cors_layer()`: production's `cors-public-api` — any origin, `GET`/`POST`/`OPTIONS`, `content-type`/`accept`, max-age 600, no credentials. |
| `ip_extract` | `IpExtractor` checks proxy headers (X-Real-IP, X-Forwarded-For) only when the peer IP is in the trusted proxy CIDR list; `CF-Connecting-IP` is ignored (no Cloudflare in front of any service). Safe default: empty list ignores all headers. |
| `error` | `ApiError` trait + `into_error_response()` produces `{"error": {"code": "...", "message": "..."}}` JSON. Adds `Retry-After` header for rate-limited responses. |
| `rate_limit` | `check_keyed_cost` and `check_direct_cost` wrap governor's GCRA limiter. Emit `{prefix}_rate_limit_hits_total` metrics on rejection. |
| `security_headers` | `security_headers_layer()` returns an axum middleware closure that emits production's `secure-headers` and `csp-tool-spa` itself: CSP, HSTS (`SecurityHeadersConfig.hsts`, default one year with `preload`), X-Content-Type-Options, X-Frame-Options, Referrer-Policy, Permissions-Policy, COOP, CORP `cross-origin`. Relaxed CSP for `/docs` only. Parity rule: [`specs/rules/architecture-rules.md`](../../specs/rules/architecture-rules.md) §Security Headers. |
| `telemetry` | `init_subscriber()` sets up tracing-subscriber with env filter + optional OTel OTLP layer. `TelemetryConfig` (log_format, enabled, otlp_endpoint, service_name, sample_rate). `shutdown()` flushes spans. All tools must use this -- see [`specs/rules/logging-rules.md`](../../specs/rules/logging-rules.md). |

## Key Dependencies

- `axum` 0.8 -- HTTP types (HeaderMap, StatusCode, IntoResponse, middleware)
- `governor` 0.10 -- GCRA rate limiting
- `ip_network` 0.4 -- CIDR parsing and matching for trusted proxies
- `metrics` 0.24 -- Rate limit rejection counters
- `serde` -- JSON serialization for error responses
- `tracing` -- Structured logging

## Common Patterns

- **Safe defaults**: `IpExtractor` with no trusted proxies returns the peer IP directly, preventing IP spoofing.
- **Bare IP auto-promotion**: Individual IPs like `10.0.0.1` are promoted to `/32` (IPv4) or `/128` (IPv6) for consistent CIDR matching.
- **Right-to-left XFF walk**: `X-Forwarded-For` is walked from right to left, skipping trusted proxies, to find the real client IP.
- **Error trait pattern**: Each service defines its own error enum and implements `ApiError`. The shared `into_error_response` handles serialization.
- **Metrics prefix**: Rate limit functions take a `metrics_prefix` so each service gets distinct counter names.
