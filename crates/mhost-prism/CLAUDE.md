# CLAUDE.md — prism

## Tool-specific principles

- **Secure by Default**: This is an open DNS proxy -- security is load-bearing (see Security Checklist below).
- **Determinism**: Same input -> same output. Pin randomness in tests, avoid time-dependent logic where possible.

## Project Overview

**prism** is a web-based DNS debugging service powered by mhost-lib. It serves an embedded SPA and exposes mhost-lib's capabilities (multi-server fan-out, streaming results, DNSSEC validation) through an HTTP API with Server-Sent Events streaming.

- **Author**: Lukas Pustina | **License**: MIT / Apache-2.0
- **Repository**: Standalone repo (separate from mhost). Depends on `mhost` as a published crate (no `app` feature).
- **README**: `README.md` — user-facing docs: features, query language, all three modes, API reference, configuration, security, dev setup.
- **SDD**: `docs/done/sdd-2025-03-07.md` (historical) — the original design document. Current architecture is documented in this file (CLAUDE.md) and README.md.

Core principles: high performance, high efficiency, high stability, high security (defense-in-depth: query restrictions, rate limiting, IP extraction, security headers).

## Build & Test

Use `make` targets for all build and test operations.

```sh
# Prerequisites: Node.js (for frontend), Rust toolchain

# Full production build (frontend + backend)
make                                  # or: make all

# Individual targets
make check                            # cargo check (fast compile check)
make test                             # cargo test
make clippy                           # cargo clippy -- -D warnings
make fmt                              # cargo fmt
make fmt-check                        # cargo fmt -- --check
make lint                             # clippy + fmt-check
make frontend                         # cd frontend && npm ci && npm run build
make clean                            # remove target/ + frontend/dist/ + node_modules/
make ci                               # lint + test + frontend (CI pipeline; also use before pushing)

# Development (two terminals)
make frontend-dev                     # Vite dev server :5173 (proxies /api/* to :8080)
make dev                              # cargo run (axum server :8080)
```

### Test Guidelines

- **Query language parser** is the most critical test surface — it is the single source of truth for query semantics.
- **Query policy validation**: Unit tests for `is_allowed_target()`, blocked types, limits enforcement.
- **SSE streaming**: Integration tests via `axum::test` with mocked DNS responses (no real network).
- **Rate limiting**: Unit tests for query cost calculation (`record_types * servers`).
- **Circuit breaker**: Unit tests for state transitions (closed → open → half-open → closed).
- **Config validation**: Test startup validation — invalid values rejected, excessive values clamped.
- **Frontend tokenizer**: Unit tests (vitest) for syntax highlighting classification.

## Architecture

```
mhost-prism/                  # standalone crate (not a workspace member)
  Cargo.toml                  # depends on mhost = "<version>" (crates.io)
  prism.example.toml          # full annotated config reference (all defaults documented)
  prism.dev.toml              # relaxed limits + arbitrary servers for local development
  src/
    main.rs                   # Entry point, axum server setup, graceful shutdown,
                              #   request_id_middleware, http_metrics_middleware
    parser.rs                 # Query language parser (single source of truth for query semantics)
                              #   ParsedQuery fields: truncated_servers: bool (set when @all/@public
                              #   exceed cap), recursive: bool (false when +norecurse)
                              #   Server group aliases: @public → Google+Cloudflare+Quad9,
                              #   @cloudflare → 1.1.1.1+1.0.0.1, @google → 8.8.8.8+8.8.4.4,
                              #   @quad9 → 9.9.9.9+149.112.112.112, @all → all public (capped to 4)
    config.rs                 # config crate: TOML + env vars (PRISM_ prefix)
    error.rs                  # thiserror ApiError enum → HTTP status + error codes
    record_format.rs          # Human-readable formatting for TXT, CAA, MX, SOA, SVCB, TLSA, etc.
    telemetry.rs              # tracing-subscriber init; optional OTel OTLP export; log_format switch
    circuit_breaker.rs        # Per-provider sliding-window breaker (CircuitBreakerRegistry)
    dns_raw.rs                # Raw UDP/TCP hickory-proto queries, glue resolution, build_server_list
    dns_trace.rs              # Iterative delegation walker (root → TLD → authoritative)
    dns_dnssec.rs             # DNSSEC chain-of-trust fetch helpers
    ip_enrichment.rs          # IpEnrichmentService: reqwest + moka cache, batch lookups
    reload.rs                 # SIGHUP hot config reload via ArcSwap
    result_cache.rs           # LRU result cache for permalink sharing
    api/
      mod.rs                  # Route definitions, AppState, shared BatchEvent / STREAM_TIMEOUT_SECS
      query.rs                # GET/POST /api/query → SSE stream (FuturesUnordered fan-out)
      check.rs                # POST /api/check → SSE stream (15 types + DMARC lint)
                              #   Additional checks: lame delegation (AA bit per NS), delegation
                              #   consistency (parent vs. child NS diff), DNSSEC rollover detection
                              #   (multiple KSKs, orphaned DS, missing DS for new KSK), DNSKEY
                              #   algorithm security rating (RSA/MD5 and RSA/SHA-1 deprecated)
      trace.rs                # POST /api/trace → SSE stream (iterative delegation walk)
      compare.rs              # POST /api/compare → SSE stream (transport comparison)
      authcompare.rs          # POST /api/authcompare → SSE stream (auth vs recursive)
      dnssec.rs               # POST /api/dnssec → SSE stream (DNSSEC chain-of-trust)
      parse.rs                # POST /api/parse → completion hints
      results.rs              # Shared result serialisation helpers (lookups → JSON)
      meta.rs                 # GET /health (liveness), GET /ready (readiness),
                              #   GET /api/servers, GET /api/record-types, GET /api/config
                              #   GET /docs → Scalar API reference UI
                              #   GET /api-docs/openapi.json → OpenAPI spec
    security/
      mod.rs                  # Middleware composition, cors_layer, security_headers
      rate_limit.rs           # 3-tier GCRA (per-IP, per-target, global)
      ip_extract.rs           # Real client IP from CF-Connecting-IP / X-Real-IP / X-Forwarded-For
      query_policy.rs         # Target validation (is_allowed_target), type restrictions, limits
  frontend/                   # SolidJS + Vite (strict TypeScript)
    src/
      App.tsx
      components/
        QueryInput.tsx        # CodeMirror 6 single-line input with autocomplete
        ResultsTable.tsx      # Streaming results table with expand/collapse
        LintTab.tsx           # Check mode lint results with remediation hints
        TraceView.tsx         # Delegation hop visualisation
        DnssecView.tsx        # DNSSEC chain-of-trust display
        ServerComparison.tsx  # Multi-server divergence view
        TransportComparison.tsx # Transport comparison (UDP/TCP/TLS/HTTPS)
        AuthComparison.tsx    # Authoritative vs recursive comparison
      lib/
        tokenizer.ts          # Syntax highlighting (cosmetic only — never affects query execution)
      styles/                 # Plain CSS with custom properties
    dist/                     # Build output, .gitignored, embedded via rust-embed
```

**Dependency rules**:
- prism depends on `mhost` as a published crate (no `app` feature). If mhost-lib lacks needed API surface, address upstream separately.
- prism never imports CLI parsing, terminal formatting, or TUI code.
- The Rust query language parser is the **single source of truth** — the frontend never parses queries into structured JSON.
- The TypeScript tokenizer is cosmetic only (syntax highlighting) — misclassification produces wrong colors, not wrong queries.

## Common Patterns

- **SSE streaming**: Per-record-type queries via `FuturesUnordered`, all record types in parallel; each completed batch streamed as a `batch` SSE event. All streams have a hard 30s deadline. All streaming endpoints also accept `?stream=false` (or `Accept: application/json`) to collect the full stream server-side and return `{ "events": [...], "truncated": bool }` as a single JSON response.
- **Per-request ResolverGroup**: Fresh `ResolverGroup` per API request — no shared resolver pool.
- **No server-side DNS caching**: Debugging tool = fresh results. Upstream resolvers cache per TTL.
- **Query cost model**: Rate limit tokens = `record_types * servers`. Pre-check enforcement before execution. Check endpoint cost = `16 * server_count` (16 steps × number of servers). Trace endpoint cost = flat 16 tokens. Compare endpoint cost = `record_types * servers * 4` (4 transports). Auth compare cost = `record_types * servers + 16` (recursive + NS discovery + auth queries).
- **Circuit breaker**: Per-provider, shared via `Arc<CircuitBreakerRegistry>` in axum app state.
- **Config precedence**: `PRISM_CONFIG` env var or CLI arg > TOML file > built-in defaults. Env vars override TOML (`PRISM_` prefix, `__` section separator). Hardcoded caps are upper bounds that config cannot exceed. Notable options: `PRISM_SERVER__TRUSTED_PROXIES` accepts individual IPs and CIDR ranges (e.g. `["10.0.0.1", "172.16.0.0/12"]`); invalid entries are skipped with a warning at startup.
- **Routing flags**: `+check`, `+trace`, `+compare`, and `+auth` in a query string are routing hints — the frontend detects them and calls the dedicated endpoint. The backend parser accepts them silently; they do not affect query execution at `/api/query`.
- **Query flags**: `+norecurse` sets RD=0 (non-recursive query, stored as `recursive: false` on `ParsedQuery`). `+short` suppresses TTL display in output.

## Key Dependencies

### Rust
- `mhost` (crates.io) — DNS library (no `app` feature)
- `axum` 0.8 — Web framework (routes, extractors, SSE)
- `tower-http` 0.6 — CORS, compression, tracing, security headers
- `tower-governor` — Rate limiting (GCRA)
- `rust-embed` 8 — Embed frontend assets
- `tokio`, `tokio-stream` — Async runtime, `ReceiverStream` for SSE
- `config` — Layered configuration (TOML + env vars)
- `thiserror` — Structured error enums
- `uuid` (v7 feature) — Time-ordered request IDs
- `metrics` + `metrics-exporter-prometheus` — Prometheus metrics

### Frontend
- `solid-js` — Reactive UI (~7KB)
- `@codemirror/*` — Editor core, state, autocomplete, language
- `vite` + `vite-plugin-solid` — Build tooling

## Architecture Rules

Rules: [`specs/rules/architecture-rules.md`](../specs/rules/architecture-rules.md) in the netray.info meta repo. Apply when modifying health probes or readiness checks.

## Logging & Telemetry

Rules: [`specs/rules/logging-rules.md`](../specs/rules/logging-rules.md) in the netray.info meta repo. Follow those rules when modifying tracing init, log filters, or `[telemetry]` config.

Default filter: `info,prism=debug,hyper=warn,h2=warn`. Telemetry config via `[telemetry]` section or `PRISM_TELEMETRY__*` env vars. Production uses `log_format = "json"` and `service_name = "prism"`.

## CI/CD

Workflow rules: [`specs/rules/workflow-rules.md`](../specs/rules/workflow-rules.md) in the netray.info meta repo. Follow those rules when creating or modifying any `.github/workflows/*.yml` file.

Workflows: `ci.yml` (PR gate: fmt, clippy, test, frontend, audit), `release.yml` (tag-push: test → build → merge), `deploy.yml` (fires after release via webhook).

GitHub Packages auth (`NODE_AUTH_TOKEN`) requirement: see workflow-rules R-J3.

## Frontend Rules

Full spec: [`specs/rules/frontend-rules.md`](../specs/rules/frontend-rules.md) in the netray.info meta repo. Apply when modifying anything under `frontend/`.

Prism uses CodeMirror 6 for its query input -- this is specific to prism's query language and not a suite-wide requirement.

## Security Checklist

When modifying API endpoints or adding features, verify:

- [ ] Blocked query types enforced (ANY, AXFR, IXFR)
- [ ] Target IP validation (no RFC 1918, localhost, link-local, CGNAT, multicast, IPv6 ULA fc00::/7)
- [ ] Glue IPs from trace delegation walk also pass `is_allowed_target` (dns_trace.rs)
- [ ] Query limits respected (max 10 record types, max 4 servers)
- [ ] Timeouts enforced (10s per-query, 30s stream)
- [ ] Rate limiting applied with correct cost calculation
- [ ] No PII in logs (no full DNS response content)
- [ ] Security headers present on all responses
- [ ] CORS restricted to same origin
