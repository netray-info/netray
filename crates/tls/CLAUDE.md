# CLAUDE.md — tlsight

## Tool-specific principles

- **Secure by Default**: This service makes outbound TCP+TLS connections to user-specified targets -- security is load-bearing (see SDD S8).

## Project Overview

**tlsight** is a web-based TLS certificate inspection and diagnostics service — the third tool in the `*.netray.info` ecosystem (`tls.netray.info`). It serves an embedded SPA and performs TLS handshake inspection against user-specified hostnames: full certificate chain extraction, TLS parameter analysis, DNS cross-checks (DANE/TLSA, CAA), and multi-IP consistency comparison.

- **Author**: Lukas Pustina | **License**: MIT
- **Repository**: Standalone repo. Depends on `mhost` as a published crate (no `app` feature).
- **SDD**: `docs/done/sdd-2026-03-08.md` — the original design document (archived). Active design docs live in `docs/`.

Core principles: high performance, high efficiency, high stability, high security (defense-in-depth: target restrictions, rate limiting, IP extraction, security headers).

## Frontend Rules

Full spec: [`specs/rules/frontend-rules.md`](../../specs/rules/frontend-rules.md). Apply when modifying anything under `frontend/`.

See `HostInput.tsx` for the reference input pattern implementation.

## Build & Test

The verbs live in the root `justfile` (see the root `README.md`); run them from the repository root.

```sh
# Prerequisites: Node.js (for frontend), Rust toolchain; `just adlc-setup` once per checkout

just --list                           # list all recipes with descriptions

# The gate — offline, run it before every commit
just adlc-verify                      # fmt-check + clippy + tests + site checks

# Full production build (frontends + release binary `netray`)
just build

# This crate only
cargo test -p netray-tls                 # Rust tests
npm test -w tlsight-frontend          # frontend tests (vitest)
just e2e tlsight                      # Playwright E2E tests against BASE_URL

# Development (two terminals)
npm run dev -w tlsight-frontend                  # Vite dev server :5174
netray tls crates/tls/tlsight.dev.toml       # the service

# CA/CAA data (refreshes data/caa_domains.tsv — commit the result)
just tlsight-data                     # fetch SSLMate + CCADB sources and regenerate TSV
```

### Test Guidelines

- **Input parsing** is the most critical test surface — hostname:port parsing, edge cases (IPv6 brackets, trailing dots, Punycode, underscores, wildcards, IP addresses).
- **Target policy**: Unit tests for IP blocklist (RFC 1918, loopback, link-local, CGNAT).
- **Chain validation**: Unit tests with canned certificate chains (valid, expired, self-signed, missing intermediate, wrong hostname).
- **DANE matching**: Unit tests for TLSA certificate usage types (0-3), selector types (0-1), matching types (0-2).
- **CAA compliance**: Unit tests for issuer matching against CAA records.
- **Summary computation**: Unit tests verifying verdict roll-up from individual checks.
- **Rate limiting**: Unit tests for cap-and-warn IP selection (prefers one v4 + one v6).
- **Integration tests**: `axum::test` with mocked TLS connections (no real network).
- **Config validation**: Startup validation tests (clamped values, rejected zeros).
- **Frontend**: vitest for component tests (chain rendering, validation summary).

## Architecture

```
tlsight/
  Cargo.toml                      # depends on mhost (crates.io), rustls, etc.
  build.rs                        # (1) panics in release if frontend/dist missing; (2) reads data/caa_domains.tsv and generates src/validate/caa_issuers.rs into $OUT_DIR at compile time
  data/
    process.py                    # merges SSLMate JSON + CCADB CSV into caa_domains.tsv; run via `just tlsight-data` from the repository root
    caa_domains.tsv               # committed; two columns: caa_domain <TAB> ca_name (155 entries, sorted)
    sslmate_issuers.json          # gitignored; fetched from web.api.sslmate.com/caahelper/issuers
    ccadb_caa_identifiers.csv     # gitignored; fetched from CCADB AllCAAIdentifiersReportCSVV2
  src/
    main.rs                       # entry point, axum server, graceful shutdown
    config.rs                     # netray_common::config::load: TOML + env vars (TLSIGHT_ prefix), unknown keys rejected
    error.rs                      # thiserror AppError enum -> HTTP status + error codes
    input.rs                      # hostname[:port,...] input parsing and validation
    state.rs                      # AppState (config, rate limiter, dns resolver, trust store)
    routes.rs                     # axum router, endpoint handlers
    scalar_docs.html              # Scalar API docs UI template
    enrichment.rs                 # IP enrichment client (geo, ASN, rDNS via ip_api_url)
    tls/
      mod.rs                      # TLS inspection orchestration
      connect.rs                  # TCP connect + TLS handshake execution; STARTTLS upgrade (SMTP ports 25, 587); key exchange group capture
      chain.rs                    # Certificate chain parsing and validation; AIA URL extraction (OCSP responder + CA Issuers); cert policy classification (EV/OV/DV); lifetime_days field
      params.rs                   # TLS version, cipher suite, ALPN, key_exchange_group extraction
      ocsp.rs                     # OCSP staple parsing (OcspInfo); live OCSP check stub (OcspRevocationResult) — stub returns "unknown" pending full OCSP request builder
    dns/
      mod.rs                      # DNS lookups for CAA, TLSA, A/AAAA, HTTPS
      caa.rs                      # CAA record fetch + issuer cross-check
      tlsa.rs                     # TLSA record fetch + DANE validation
      https_record.rs             # HTTPS DNS record query (type 65) for ECH advertisement detection
    validate/
      mod.rs                      # Cross-validation orchestration
      chain_trust.rs              # Chain completeness, expiry, signature algo checks
      dane.rs                     # TLSA record vs. presented certificate matching
      caa_compliance.rs           # CAA record vs. issuing CA matching; uses generated caa_issuers lookup (no heuristic)
      caa_issuers.rs              # include! of $OUT_DIR/caa_issuers.rs (generated by build.rs from data/caa_domains.tsv)
      ct.rs                       # Certificate Transparency SCT extraction (optional)
    security/
      mod.rs                      # Security headers, CORS
      rate_limit.rs               # GCRA rate limiting (per-IP, per-target)
      ip_extract.rs               # Client IP from proxy headers
      target_policy.rs            # Target validation (no internal IPs, port restrictions)
    quality/
      mod.rs                      # Quality/health assessment orchestration
      checks.rs                   # Individual quality check implementations; includes cert_lifetime (398/825-day), alpn_consistency, ech_advertised, tls_version (Warn on TLS 1.2)
      http.rs                     # HTTP reachability and redirect checks
      types.rs                    # Quality check result types
  frontend/                       # SolidJS + Vite (strict TypeScript)
    src/
      index.tsx                   # SolidJS entry point (renders App)
      App.tsx                     # Main state, inspection trigger, theme
      vite-env.d.ts               # Vite client type declarations
      components/
        HostInput.tsx             # Hostname input with port selector
        ChainView.tsx             # Certificate chain visualization
        CertDetail.tsx            # Individual certificate details
        TlsParams.tsx             # Negotiated TLS parameters
        ValidationSummary.tsx     # Pass/warn/fail validation results
        ConsistencyView.tsx       # Multi-IP consistency comparison
        CtView.tsx                # Certificate Transparency SCT display
        DnsInfo.tsx               # CAA and TLSA DNS cross-check results
        IpBadges.tsx              # IP address badge display
        IpCard.tsx                # Per-IP inspection result card
        UnifiedIpView.tsx         # Unified multi-IP result view
        PortTabs.tsx              # Multi-port tab navigation
        QueryHistory.tsx          # Recent query history (localStorage)
        ExportButtons.tsx         # JSON download + markdown copy
        CrossLinks.tsx            # Links to dns.netray.info and ip.netray.info
        Explain.tsx               # Reusable explain-mode info card wrapper
      lib/
        types.ts                  # TypeScript interfaces matching Rust response
        api.ts                    # API client for /api/inspect
        cert.ts                   # Certificate utility functions
        history.ts                # Query history management (localStorage)
        trust.ts                  # Trust store / root CA utilities
      styles/
        global.css                # Plain CSS with custom properties
    dist/                         # Build output, .gitignored, embedded via rust-embed
  docs/
    sdd.md                        # Software Design Document
```

**Dependency rules**:
- tlsight depends on `mhost` as a published crate (no `app` feature). If mhost-lib lacks needed API surface, address upstream separately.
- tlsight never imports CLI parsing, terminal formatting, or TUI code from mhost.
- The `AcceptAnyCert` verifier is used **only** for inspection connections, never for internal HTTPS or DNS-over-HTTPS.

## Common Patterns

- **Synchronous JSON**: No SSE — a TLS handshake completes in milliseconds. Standard request/response with structured JSON.
- **Inspection pipeline**: Resolve IPs → filter blocked → cap-and-warn → concurrent handshakes (semaphore-bounded) + concurrent DNS (CAA, TLSA) → cross-validate → summary verdict.
- **Per-request concurrency**: `JoinSet` + `Arc<Semaphore>` bounds concurrent handshakes per request (`max_concurrent_handshakes`). Ports run concurrently, not sequentially.
- **Cap-and-warn rate limiting**: When multi-IP fan-out exceeds rate budget, reduce inspected IPs (prefer one v4 + one v6) instead of rejecting. Response includes `warnings` and `skipped_ips`.
- **Trust store**: `RootCertStore` built at startup from `webpki-roots` (Mozilla bundle) + all `*.pem` and `*.crt` files from `custom_ca_dir` (if configured). Supports private CAs without rebuilds.
- **Config precedence**: CLI arg / `TLSIGHT_CONFIG` env var > TOML file > built-in defaults. Env vars override TOML (`TLSIGHT_` prefix, `__` section separator). Hardcoded caps (§8.1) are upper bounds that config cannot exceed. Every config struct is `deny_unknown_fields`; `netray tls --check-config <path>` validates a file, including that `custom_ca_dir` exists, and exits 0 (`config ok: <path>`) or 1 with the error.
- **Error responses**: Structured JSON via `AppError` enum: `{ "error": { "code": "...", "message": "..." } }`.
- **Request IDs**: UUID v7 in `X-Request-Id` header on every response.
- **Static file serving**: `rust-embed` in release, filesystem reads in debug. Vite-hashed assets get `immutable` cache headers; `index.html` gets `no-cache`.
- **Prometheus metrics**: Separate port. `metrics` macros are no-op when no recorder is installed (safe in tests).
- **CAA issuer matching**: `caa_compliance::issuer_domain_matches` looks up the CAA `issue` domain in the compile-time table (`caa_issuers::lookup_caa_issuer`, binary search). If found, it checks the cert's issuer DN O=/CN= fields against the CA name using bidirectional normalized containment and ≥6-char word overlap. Unknown CAA domains → Fail. Refresh the table with `just tlsight-data` when CAs are added or renamed.

## Key Dependencies

### Rust
- `mhost` (crates.io, no `app`) — DNS resolution (CAA, TLSA, A/AAAA lookups)
- `rustls` + `tokio-rustls` — TLS handshake execution (pure-Rust, async)
- `webpki` + `webpki-roots` — Trust store chain validation (Mozilla root bundle + custom CA directory)
- `x509-parser` — Certificate chain parsing (DER/PEM, extensions, OIDs)
- `x509-ocsp` — OCSP stapled response parsing
- `axum` 0.8 — Web framework (routes, extractors, JSON)
- `tower-http` 0.6 — CORS, compression, tracing, security headers
- `governor` 0.8 — Rate limiting (GCRA, per-IP + per-target)
- `rust-embed` 8 — Embed frontend assets
- `config` — Layered configuration (TOML + env vars)
- `thiserror` — Structured error enums
- `utoipa` — OpenAPI spec + Scalar docs UI
- `tokio` — Async runtime
- `uuid` (v7 feature) — Time-ordered request IDs
- `serde` + `serde_json` — Serialization
- `metrics` + `metrics-exporter-prometheus` — Prometheus metrics

### Frontend
- `solid-js` — Reactive UI (~7KB)
- `vite` + `vite-plugin-solid` — Build tooling

## Architecture Rules

Rules: [`specs/rules/architecture-rules.md`](../../specs/rules/architecture-rules.md). Apply when modifying health probes or readiness checks.

## Logging & Telemetry

Rules: [`specs/rules/logging-rules.md`](../../specs/rules/logging-rules.md). Follow those rules when modifying tracing init, log filters, or `[telemetry]` config.

Default filter: `info,netray_tls=debug,hyper=warn,h2=warn`. Telemetry config via `[telemetry]` section or `TLSIGHT_TELEMETRY__*` env vars. Production uses `log_format = "json"` and `service_name = "tlsight"`.

## CI/CD

Workflow rules: [`specs/rules/workflow-rules.md`](../../specs/rules/workflow-rules.md). Follow those rules when creating or modifying any `.github/workflows/*.yml` file.

Workflows: `ci.yml` (PR gate: fmt, clippy, test, frontend, deny), `audit.yml` (daily advisory scans: RUSTSEC, npm audit), `release.yml` (tag-push: test → build → merge), `deploy.yml` (fires after release via webhook).

## Security Checklist

When modifying API endpoints or adding features, verify:

- [ ] Target IP validation enforced (no RFC 1918, localhost, link-local, CGNAT, multicast)
- [ ] DNS rebinding mitigated (resolved IP checked before connect, no re-resolution)
- [ ] Port limits respected (max 7 per request, 1-65535)
- [ ] IP-per-hostname limit respected (max 10)
- [ ] Timeouts enforced (5s per-handshake, 15s per-request)
- [ ] Rate limiting applied with correct cost calculation (ports * inspected_ips)
- [ ] `AcceptAnyCert` verifier used only for inspection connections
- [ ] STARTTLS targets pass the same target policy validation (RFC 1918 / loopback blocklist) as direct TLS targets
- [ ] No application data sent after TLS handshake (handshake only, then close)
- [ ] No PII in logs (no full certificate content)
- [ ] Security headers present on all responses
- [ ] CORS is production's public-API set: `Access-Control-Allow-Origin: *`, no credentials
- [ ] Custom CA directory loads only `*.pem` and `*.crt` files, fails fast on bad PEM
