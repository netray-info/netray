# CLAUDE.md — tlsight

## Rules

- Do NOT add a `Co-Authored-By` line for Claude in commit messages.
- Don't add heavy dependencies for minor convenience — check if existing deps already cover the need.
- Don't mix formatting-only changes with functional changes in the same commit.
- Don't modify unrelated modules "while you're in there" — keep changes scoped.
- Don't add speculative flags, config options, or abstractions without a current caller.
- Don't bypass failing checks (`--no-verify`, `#[allow(...)]`) without explaining why.
- Don't hide behavior changes inside refactor commits — separate them.
- Don't include PII, real email addresses, or real domains (other than example.com) in test data, docs, or commits.
- If uncertain about an implementation detail, leave a concrete `TODO("reason")` rather than a hidden guess.

## Engineering Principles

- **Performance**: Prioritize efficient algorithms and data structures. Avoid unnecessary allocations and copies.
- **Rust patterns**: Use idiomatic Rust constructs (enums, traits, iterators) for clarity and safety. Leverage type system to prevent invalid states.
- **KISS**: Simplest solution that works. Three similar lines beat a premature abstraction.
- **YAGNI**: Don't build for hypothetical future requirements — solve the current problem.
- **DRY + Rule of Three**: Tolerate duplication until the third occurrence, then extract.
- **SRP**: Each module/struct has one reason to change. Split when responsibilities diverge.
- **Fail Fast**: Validate at boundaries, return errors early, don't silently swallow failures.
- **Secure by Default**: Sanitize external input, no PII in logs, prefer safe APIs. This service makes outbound TCP+TLS connections to user-specified targets — security is load-bearing (see SDD §8).
- **Reversibility**: Prefer changes that are easy to undo. Small commits over monolithic ones.

## Project Overview

**tlsight** is a web-based TLS certificate inspection and diagnostics service — the third tool in the `*.pdt.sh` ecosystem (`tls.pdt.sh`). It serves an embedded SPA and performs TLS handshake inspection against user-specified hostnames: full certificate chain extraction, TLS parameter analysis, DNS cross-checks (DANE/TLSA, CAA), and multi-IP consistency comparison.

- **Author**: Lukas Pustina | **License**: MIT
- **Repository**: Standalone repo. Depends on `mhost` as a published crate (no `app` feature).
- **SDD**: `docs/sdd.md` — the authoritative design document for all architecture decisions.

Core principles:

- high performance
- high efficiency
- high stability
- high security (defense-in-depth: target restrictions, rate limiting, IP extraction, security headers)

## Design Document

The Software Design Document (`docs/sdd.md`) is the source of truth for architecture, API design, security model, and phased delivery. Always consult it before making design decisions. Key sections:

- **§4** Input language (hostname[:port[,port...]])
- **§5** API endpoints (inspect, health, ready, meta, OpenAPI)
- **§7** Backend architecture (axum, inspection pipeline, TLS handshake)
- **§8** Security architecture (4-layer defense-in-depth)
- **§9** Configuration (TOML + env vars)
- **§14** Phased delivery plan

## Technology Decisions

Decisions made during project setup, supplementing the SDD:

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Repository** | Standalone repo, `mhost` via crates.io | Independent release cadence; same pattern as prism/ifconfig-rs |
| **TLS implementation** | `rustls` (pure Rust) | No OpenSSL dependency, static musl builds, memory-safe. Trade-off: no legacy cipher support (RC4, 3DES) |
| **Certificate verifier** | `AcceptAnyCert` custom verifier | Inspection tool must see broken certs, not reject them. Validation is application-level, post-handshake |
| **Trust store** | `webpki-roots` + optional CA directory (`custom_ca_dir` config) | Mozilla bundle as baseline; operators drop PEM files into a directory for private CAs. All `*.pem` files loaded at startup. Supports internal PKI (e.g., Step-CA) without rebuilds |
| **Cert parsing** | `x509-parser` + `x509-ocsp` | DER/PEM parsing, extension extraction, OID mapping. `x509-ocsp` for OCSP staple parsing |
| **DNS resolution** | `mhost` (not `hickory-resolver`) | Ecosystem consistency, proven DNSSEC support, same-team maintenance. Heavier than needed but accepted cost |
| **Response model** | Synchronous JSON (not SSE) | TLS handshakes complete in milliseconds; SSE is unnecessary overhead |
| **Rate limiting** | `tower-governor` (GCRA), two-tier + cap-and-warn | Per-source-IP + per-target-hostname. Multi-IP fan-out degrades gracefully instead of rejecting |
| **CSS** | Plain CSS with custom properties | Small frontend, zero build config, maps to ecosystem design language |
| **Config parsing** | `config` crate | Same pattern as prism/ifconfig-rs: TOML + `TLSIGHT_` prefix + `__` separators |
| **Error handling** | `thiserror` | Structured `AppError` enum maps to HTTP status + error codes |
| **Request IDs** | `uuid` crate with `v7` feature | Time-ordered UUIDs, same as prism |
| **TypeScript** | Strict mode | Low ceremony cost, catches bugs at compile time |
| **Input** | Plain `<input>`, no CodeMirror | No query language — just `hostname[:port[,port...]]`. CodeMirror is overkill |

## Build & Test

**Always use `make` targets** — never run raw `cargo`, `npm`, or `npx` commands directly.

```sh
# Prerequisites: Node.js (for frontend), Rust toolchain

make help                             # list all targets with descriptions

# Full production build (frontend + backend)
make                                  # or: make all
make run                              # build + run release binary

# Rust
make check                            # cargo check (fast compile check)
make test-rust                        # cargo test
make clippy                           # cargo clippy -- -D warnings
make fmt                              # cargo fmt
make fmt-check                        # cargo fmt -- --check

# Frontend
make frontend-install                 # npm ci (deps only, no build)
make frontend                         # npm ci + npm run build
make frontend-test                    # npm ci + vitest run

# Combined
make test                             # test-rust + test-frontend
make lint                             # clippy + fmt-check
make ci                               # lint + test + frontend (run before pushing)

# Development (two terminals)
make frontend-dev                     # Vite dev server :5173 (proxies /api/* to :8080)
make dev                              # cargo run with tlsight.dev.toml

# Cleanup
make clean                            # remove target/ + frontend/dist/ + node_modules/
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
  build.rs                        # panics in release if frontend/dist missing
  Makefile                        # build/test/docker targets
  src/
    main.rs                       # entry point, axum server, graceful shutdown
    config.rs                     # config crate: TOML + env vars (TLSIGHT_ prefix)
    error.rs                      # thiserror AppError enum -> HTTP status + error codes
    input.rs                      # hostname[:port,...] input parsing and validation
    state.rs                      # AppState (config, rate limiter, dns resolver, trust store)
    routes.rs                     # axum router, endpoint handlers
    scalar_docs.html              # Scalar API docs UI template
    tls/
      mod.rs                      # TLS inspection orchestration
      connect.rs                  # TCP connect + TLS handshake execution
      chain.rs                    # Certificate chain parsing and validation
      params.rs                   # TLS version, cipher suite, ALPN extraction
      ocsp.rs                     # OCSP stapling check
    dns/
      mod.rs                      # DNS lookups for CAA, TLSA, A/AAAA
      caa.rs                      # CAA record fetch + issuer cross-check
      tlsa.rs                     # TLSA record fetch + DANE validation
    validate/
      mod.rs                      # Cross-validation orchestration
      chain_trust.rs              # Chain completeness, expiry, signature algo checks
      dane.rs                     # TLSA record vs. presented certificate matching
      caa_compliance.rs           # CAA record vs. issuing CA matching
      ct.rs                       # Certificate Transparency SCT extraction (optional)
    security/
      mod.rs                      # Security headers, CORS
      rate_limit.rs               # GCRA rate limiting (per-IP, per-target)
      ip_extract.rs               # Client IP from proxy headers
      target_policy.rs            # Target validation (no internal IPs, port restrictions)
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
        PortTabs.tsx              # Multi-port tab navigation
        QueryHistory.tsx          # Recent query history (localStorage)
        ExportButtons.tsx         # JSON download + markdown copy
        CrossLinks.tsx            # Links to dns.pdt.sh and ip.pdt.sh
      lib/
        types.ts                  # TypeScript interfaces matching Rust response
        api.ts                    # API client for /api/inspect
        history.ts                # Query history management (localStorage)
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
- **Trust store**: `RootCertStore` built at startup from `webpki-roots` (Mozilla bundle) + all `*.pem` files from `custom_ca_dir` (if configured). Supports private CAs without rebuilds.
- **Config precedence**: CLI arg / `TLSIGHT_CONFIG` env var > TOML file > built-in defaults. Env vars override TOML (`TLSIGHT_` prefix, `__` section separator). Hardcoded caps (§8.1) are upper bounds that config cannot exceed.
- **Error responses**: Structured JSON via `AppError` enum: `{ "error": { "code": "...", "message": "..." } }`.
- **Request IDs**: UUID v7 in `X-Request-Id` header on every response.
- **Static file serving**: `rust-embed` in release, filesystem reads in debug. Vite-hashed assets get `immutable` cache headers; `index.html` gets `no-cache`.
- **Prometheus metrics**: Separate port. `metrics` macros are no-op when no recorder is installed (safe in tests).

## Key Dependencies

### Rust
- `mhost` (crates.io, no `app`) — DNS resolution (CAA, TLSA, A/AAAA lookups)
- `rustls` + `tokio-rustls` — TLS handshake execution (pure-Rust, async)
- `webpki` + `webpki-roots` — Trust store chain validation (Mozilla root bundle + custom CA directory)
- `x509-parser` — Certificate chain parsing (DER/PEM, extensions, OIDs)
- `x509-ocsp` — OCSP stapled response parsing
- `axum` 0.8 — Web framework (routes, extractors, JSON)
- `tower-http` 0.6 — CORS, compression, tracing, security headers
- `tower-governor` — Rate limiting (GCRA, per-IP + per-target)
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

## Security Checklist

When modifying API endpoints or adding features, verify:

- [ ] Target IP validation enforced (no RFC 1918, localhost, link-local, CGNAT, multicast)
- [ ] DNS rebinding mitigated (resolved IP checked before connect, no re-resolution)
- [ ] Port limits respected (max 5 per request, 1-65535)
- [ ] IP-per-hostname limit respected (max 10)
- [ ] Timeouts enforced (5s per-handshake, 15s per-request)
- [ ] Rate limiting applied with correct cost calculation (ports * inspected_ips)
- [ ] `AcceptAnyCert` verifier used only for inspection connections
- [ ] No application data sent after TLS handshake (handshake only, then close)
- [ ] No PII in logs (no full certificate content)
- [ ] Security headers present on all responses
- [ ] CORS restricted to configured origins
- [ ] Custom CA directory loads only `*.pem` files, fails fast on bad PEM
