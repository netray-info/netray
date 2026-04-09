# Logging & Telemetry Rules -- netray.info Suite

Canonical rules for logging, tracing, and telemetry configuration across all
backend services. Apply these rules when creating or modifying telemetry setup
in any tool, or when configuring production deployments.

Services in scope: `ifconfig-rs`, `mhost-prism`, `tlsight`, `spectra`, `lens`.

---

## A. Code-Level Rules

### A1. Use the shared telemetry initialiser

All tools MUST use `netray_common::telemetry::init_subscriber()` for tracing
initialisation. Direct `tracing_subscriber::fmt()` setup is not allowed.

This ensures every service gets consistent JSON output, env-filter handling,
and optional OTel export without per-tool reimplementation.

```rust
netray_common::telemetry::init_subscriber(&config.telemetry, DEFAULT_LOG_FILTER);
```

### A2. TelemetryConfig in every tool

All tools MUST include a `[telemetry]` section in their config struct,
deserialized as `netray_common::telemetry::TelemetryConfig`. The section MUST
be documented in the tool's example TOML config (`*.example.toml`).

Minimal example TOML documentation:

```toml
[telemetry]
# Log output format.
#   "text" -- human-readable, colour-coded (default; best for terminals)
#   "json" -- structured JSON lines (best for log aggregators: Loki, Dozzle, etc.)
# Also configurable via: {PREFIX}_TELEMETRY__LOG_FORMAT=json
#log_format = "text"

# Enable OpenTelemetry distributed tracing (OTLP/HTTP export).
#enabled = false

# OTLP HTTP endpoint to export spans to.
#otlp_endpoint = "http://localhost:4318"

# Service name reported in spans and logs.
#service_name = "{tool}"

# Trace sampling rate (0.0 = off, 1.0 = 100%).
#sample_rate = 1.0
```

### A3. Default log filter

The hardcoded fallback filter (used when `RUST_LOG` is not set) MUST follow
this pattern:

```
info,{crate}=debug,hyper=warn,h2=warn
```

Where `{crate}` is the tool's own crate name (e.g. `lens`, `prism`, `tlsight`).

Rationale:
- **`info` global base**: Ensures warnings and info from dependencies
  (governor, reqwest, tower, axum, etc.) are never silently dropped.
- **`{crate}=debug`**: Convenient for local development without setting
  `RUST_LOG`. Production overrides this via explicit `RUST_LOG`.
- **`hyper=warn,h2=warn`**: Suppresses noisy HTTP/2 frame-level and
  connection-level logs that provide no operational value at `info`.

ifconfig-rs note: The existing `mhost=warn` suppression should be retained
alongside the standard pattern if the mhost dependency is present.

### A4. TraceLayer at INFO level

All tools MUST configure `tower_http::trace::TraceLayer` so that both the
request span and the response completion event are at INFO level. The default
`TraceLayer::new_for_http()` uses DEBUG for both, which is invisible in
production (`RUST_LOG=info`).

Required pattern:

```rust
TraceLayer::new_for_http()
    .make_span_with(|req: &axum::http::Request<_>| {
        let request_id = req
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        tracing::info_span!(
            "http_request",
            method = %req.method(),
            uri = %req.uri(),
            request_id = %request_id,
            client_ip = tracing::field::Empty,
        )
    })
    .on_response(
        |response: &axum::http::Response<_>,
         latency: std::time::Duration,
         span: &tracing::Span| {
            tracing::info!(
                parent: span,
                status = response.status().as_u16(),
                ms = latency.as_millis(),
                "",
            );
        },
    )
```

Do NOT use `DefaultOnResponse::new().level(Level::DEBUG)` or the bare
`TraceLayer::new_for_http()` default -- both produce DEBUG events that are
filtered out in production.

### A5. Metrics port

All tools MUST default to `127.0.0.1:9090` for the metrics/admin bind address.

### A6. Graceful shutdown

All tools MUST call `netray_common::telemetry::shutdown()` in their shutdown
path. This flushes pending OTel spans before the process exits.

```rust
// In the shutdown signal handler, after server graceful shutdown:
netray_common::telemetry::shutdown();
```

### A7. Service name

All tools MUST set a non-empty default for `service_name` in their config
defaults or example config. Use the canonical short name: `ifconfig`, `prism`,
`tlsight`, `lens`.

---

## B. Production Configuration Rules (argus-oci)

### B1. Log format

All services MUST set `log_format = "json"` in their production TOML config
(or via `{PREFIX}_TELEMETRY__LOG_FORMAT=json`).

Structured JSON logs are required for Dozzle filtering and any future log
aggregation. Text format is for local development only.

### B2. RUST_LOG

All services MUST set `RUST_LOG` explicitly in the Docker Compose environment.

```yaml
environment:
  RUST_LOG: "info,hyper=warn,h2=warn"
```

This makes the production log level visible in the deployment config and
tunable without rebuilding. The tool's own crate runs at `info` (not `debug`)
in production to avoid excessive output.

### B3. Service name

All services MUST set `service_name` in the production `[telemetry]` config:

| Service | `service_name` |
|---------|---------------|
| ifconfig-rs | `ifconfig` |
| mhost-prism | `prism` |
| tlsight | `tlsight` |
| lens | `lens` |

### B4. Docker log rotation

All service containers MUST configure the `json-file` log driver with
rotation to prevent unbounded disk growth:

```yaml
logging:
  driver: json-file
  options:
    max-size: "50m"
    max-file: "5"
```

This caps each service at ~250 MB of retained logs.

### B5. OpenTelemetry

OTel export remains disabled (`enabled = false`) until a collector is deployed.
Do not set `otlp_endpoint` or `sample_rate` until then.

When OTel is enabled in future, use `sample_rate = 0.1` (10%) as the starting
point to limit storage and CPU overhead on the single-VM deployment.

---

## C. Instrumentation Rules

Rules for what to log inside the Rust codebases. The goal is production
debuggability: an operator using Dozzle (or any log viewer) should be able
to diagnose why a request failed, which backend is slow, and whether
abuse is occurring -- without reading source code.

**Principles:**

- Logs are for operators, metrics are for dashboards. Neither replaces the
  other. A rate limit rejection needs a log line (operator searches for
  "why was this client blocked?") AND a metric (alert on spike).
- Every rejected request must leave a trace. If the service returns 4xx/5xx,
  a log line must exist that an operator can find.
- Inter-service calls are the #1 debugging blind spot. When lens calls prism
  and gets nothing back, the operator needs to know: which backend, what URL,
  how long, what failed.
- DEBUG is free in production (because `RUST_LOG=info` filters it out).
  Instrument liberally at DEBUG; be selective at INFO/WARN.

### C1. Request span enrichment

All tools MUST record `request_id` and `client_ip` into the current tracing
span early in request handling. This ensures every log line emitted during
that request -- including from dependencies and downstream calls -- carries
correlation fields automatically.

```rust
let span = tracing::Span::current();
span.record("request_id", &request_id);
span.record("client_ip", &client_ip);
```

### C2. Rejection logging

Every request rejection (4xx/5xx) MUST be logged. The log level depends on
the cause:

| Cause | Level | Required fields |
|-------|-------|-----------------|
| Rate limited (429) | WARN | `client_ip`, `scope` (per-ip / per-target / global) |
| Blocked target (403/400) | WARN | `client_ip`, target |
| Input validation (400) | DEBUG | input summary (domain/IP, truncated) |
| Internal error (500) | ERROR | error detail |
| Upstream failure (502/504) | WARN | backend service, error detail |
| Not found (404) | -- | (no log -- crawler noise) |

Rationale: Rate limits and blocked targets at WARN because they indicate
potential abuse and are low-volume enough to log. Validation errors at DEBUG
because they are high-volume and usually benign user typos. 404s excluded
because bots generate millions.

### C3. Inter-service call instrumentation

Every outbound HTTP call to another netray service MUST be wrapped in a
tracing span that captures:

- `service`: target service name (`ifconfig`, `prism`, `tlsight`)
- `url`: request URL
- Duration (automatic via span lifetime)
- Outcome: success status, timeout, HTTP error, network error

Failures MUST be logged at WARN. Successes SHOULD be logged at DEBUG.

```rust
// Success
tracing::debug!(service = "ifconfig", url = %url, status = %status, "enrichment call");

// Failure
tracing::warn!(service = "ifconfig", url = %url, error = %e, "enrichment call failed");
```

**Ownership:**

- `netray_common::enrichment::EnrichmentClient` instruments its own calls.
  This covers prism and tlsight automatically.
- lens instruments its own backend calls in `backends/` using the same field
  conventions. Lens knows the service names; a generic library does not.

### C4. Startup inventory

On startup, every tool MUST log an INFO-level summary of its running
configuration. An operator reading the first few log lines should know:

- Bind addresses (server + metrics/admin)
- Which optional features are enabled/disabled
- Backend URLs (for tools that call other services)
- Rate limit parameters (per-IP rate, burst)
- Trusted proxy count (not the CIDRs themselves)

A single structured log line or a small group of related lines is fine.
Do not dump the entire config -- just the operationally relevant fields.

### C5. Telemetry self-announcement

`netray_common::telemetry::init_subscriber()` MUST log an INFO message after
successful initialisation confirming:

- Log format (`text` or `json`)
- OTel status (`enabled` with endpoint, or `disabled`)
- Service name (if non-empty)

This is the first log line the operator sees. It confirms that the logging
system itself is working as configured.

### C6. Error handler catch-all

Each tool's `impl IntoResponse for AppError` (or equivalent error-to-response
conversion) MUST log errors as a catch-all safety net:

- 5xx: ERROR level with error detail
- 4xx: per the C2 table

Even if a handler already logged the error, the catch-all ensures no
rejection goes unrecorded. Duplicate log lines (handler + catch-all) are
acceptable -- missing lines are not.

### C7. What NOT to log

- **Request/response bodies.** Size is fine; content is not.
- **Full payloads.** No certificate chains, DNS record sets, enrichment
  JSON, or scoring details at INFO. These belong in the API response.
- **PII.** No full User-Agent strings at INFO (DEBUG is acceptable). No
  client IPs in request bodies or error messages returned to users.
- **Health/readiness probes.** These fire every 30s per container. Do not
  log probe requests or responses at any level.
- **Successful cache operations** at INFO. Use DEBUG or metrics.
- **Per-record classification decisions** at INFO. Use DEBUG. An IP being
  classified as VPN/cloud/C2 is interesting for debugging but too
  high-volume for production INFO.
- **Redundant context.** Do not repeat fields that the tracing span already
  carries (request_id, client_ip) in the log message string.

### C8. Structured fields over string interpolation

Log statements MUST use tracing's structured fields, not string formatting:

```rust
// Good -- fields are queryable in JSON output
tracing::warn!(client_ip = %ip, scope = "per_ip", "rate limited");

// Bad -- fields are buried in the message string
tracing::warn!("rate limited: ip={}, scope=per_ip", ip);
```

Structured fields are filterable in Dozzle, queryable in log aggregators,
and indexable by future tooling.

---

## D. Resolved Gaps

### D1. Configuration gaps (resolved 2026-04-08)

All tools now use `netray_common::telemetry::init_subscriber()`, have
`[telemetry]` config sections, standardised default filters, and metrics
on port 9090. Production configs set `log_format = "json"`, `RUST_LOG`,
`service_name`, and Docker log rotation.
