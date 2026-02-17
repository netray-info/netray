# CLAUDE.md — prism

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

- **Performance**: Prioritize efficient algorithms and data structures. Benchmark critical paths, avoid unnecessary allocations and copies.
- **Rust patterns**: Use idiomatic Rust constructs (enums, traits, iterators) for clarity and safety. Leverage type system to prevent invalid states.
- **KISS**: Simplest solution that works. Three similar lines beat a premature abstraction.
- **YAGNI**: Don't build for hypothetical future requirements — solve the current problem.
- **DRY + Rule of Three**: Tolerate duplication until the third occurrence, then extract.
- **SRP**: Each module/struct has one reason to change. Split when responsibilities diverge.
- **Fail Fast**: Validate at boundaries, return errors early, don't silently swallow failures.
- **Secure by Default**: Sanitize external input, no PII in logs, prefer safe APIs. This is an open DNS proxy — security is load-bearing (see SDD §8).
- **Determinism**: Same input → same output. Pin randomness in tests, avoid time-dependent logic where possible.
- **Reversibility**: Prefer changes that are easy to undo. Feature flags over big-bang migrations, small commits over monolithic ones.

## Project Overview

**prism** is a web-based DNS debugging service powered by mhost-lib. It serves an embedded SPA and exposes mhost-lib's capabilities (multi-server fan-out, streaming results, DNSSEC validation) through an HTTP API with Server-Sent Events streaming.

- **Author**: Lukas Pustina | **License**: MIT / Apache-2.0
- **Repository**: Standalone repo (separate from mhost). Depends on `mhost` as a published crate (no `app` feature).
- **SDD**: `docs/sdd.md` — the authoritative design document for all architecture decisions. Note: the SDD was written assuming a workspace member; this repo diverges to a standalone crate with a crates.io dependency. Structural references (§3 workspace layout) are adapted accordingly.

prism provides and all functionality must adhere to these core principles:

- high performance
- high efficiency
- high stability
- high security (defense-in-depth: query restrictions, rate limiting, IP extraction, security headers)

## Design Document

The Software Design Document (`docs/sdd.md`) is the source of truth for architecture, API design, security model, and phased delivery. Always consult it before making design decisions. Key sections:

- **§4** Query language syntax and semantics
- **§5** API endpoints (query, check, trace, parse, metadata)
- **§7** Backend architecture (axum, SSE streaming, FuturesUnordered pattern)
- **§8** Security architecture (4-layer defense-in-depth)
- **§9** Configuration (TOML + env vars)
- **§14** Phased delivery plan

## Technology Decisions

Decisions made during project setup, supplementing the SDD:

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Repository** | Standalone repo, `mhost` via crates.io | Independent release cadence; gaps in mhost-lib addressed upstream separately |
| **CSS** | Plain CSS with custom properties | Frontend is ~3 components; custom properties map directly to mhost's color palette and dark mode toggle; zero build config |
| **Config parsing** | `config` crate | Built-in layering (TOML file + env vars + defaults); handles `PRISM_` prefix and `__` section separators natively |
| **Error handling** | `thiserror` | Structured `ApiError` enum maps to specific HTTP status + error codes (§5.6); no need for `anyhow`'s erased types |
| **Request IDs** | `uuid` crate with `v7` feature | Time-ordered UUIDs per SDD §8.5; universally recognized in headers/logs/JSON |
| **TypeScript** | Strict mode (`strict: true`) | Frontend is thin — low ceremony cost, catches bugs at compile time |

## Roadmap

Phased delivery as defined in SDD §14:

- ~~**Phase 0**: Workspace conversion (mhost repo)~~ — N/A (standalone repo)
- **Phase 1**: MVP — query endpoint, parser, results table, rate limiting, circuit breaker, metrics
- **Phase 1.5**: Library extraction (lint/trace logic from app/ to library)
- **Phase 2**: Check + trace endpoints, transport flags, DNSSEC, server comparison
- **Phase 3**: Polish — server-side autocomplete, keyboard shortcuts, history, mobile
- **Phase 4**: Future — auth, DNSSEC visualization, propagation map

## Build & Test

```sh
# Prerequisites: Node.js (for frontend), Rust toolchain

# Development (two terminals)
cd frontend && npm run dev            # Vite dev server :5173 (proxies /api/* to :8080)
cargo run                             # axum server :8080, reads frontend from disk

# Production build
cd frontend && npm ci && npm run build  # outputs to frontend/dist/
cargo build --release                   # rust-embed bakes dist/ into binary

# Testing
cargo test                            # All tests
cargo clippy                          # Lint
cargo fmt -- --check                  # Format check

# Full build (sequences frontend + backend)
just build-web                        # or: cd frontend && npm ci && npm run build && cd .. && cargo build --release
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
  src/
    main.rs                   # Entry point, axum server setup, graceful shutdown
    api/
      mod.rs                  # Route definitions
      query.rs                # GET/POST /api/query → SSE stream
      check.rs                # POST /api/check → SSE stream (Phase 2)
      trace.rs                # POST /api/trace → SSE stream (Phase 2)
      parse.rs                # POST /api/parse → completion hints (Phase 3)
      meta.rs                 # GET /api/servers, /api/record-types, /api/health
    security/
      mod.rs                  # Middleware composition
      rate_limit.rs           # tower-governor layers (per-IP, per-target, global)
      ip_extract.rs           # Real client IP from proxy headers
      query_policy.rs         # Target validation, type restrictions
    config.rs                 # config crate: TOML + env vars (PRISM_ prefix)
    error.rs                  # thiserror ApiError enum → HTTP status + error codes
  frontend/                   # SolidJS + Vite (strict TypeScript)
    src/
      App.tsx
      components/
        QueryInput.tsx        # CodeMirror 6 single-line input
        ResultsTable.tsx      # Streaming results table
        ServerComparison.tsx
      lib/
        tokenizer.ts          # Syntax highlighting (cosmetic only)
      styles/                 # Plain CSS with custom properties
    dist/                     # Build output, .gitignored, embedded via rust-embed
```

**Dependency rules**:
- prism depends on `mhost` as a published crate (no `app` feature). If mhost-lib lacks needed API surface, address upstream separately.
- prism never imports CLI parsing, terminal formatting, or TUI code.
- The Rust query language parser is the **single source of truth** — the frontend never parses queries into structured JSON.
- The TypeScript tokenizer is cosmetic only (syntax highlighting) — misclassification produces wrong colors, not wrong queries.

## Common Patterns

- **SSE streaming**: Per-record-type queries via `FuturesUnordered`, each completed batch streamed as an SSE event (mirrors mdive's pattern).
- **Per-request ResolverGroup**: Fresh `ResolverGroup` per API request — no shared resolver pool.
- **No server-side DNS caching**: Debugging tool = fresh results. Upstream resolvers cache per TTL.
- **Query cost model**: Rate limit tokens = `record_types * servers`. Pre-check enforcement before execution.
- **Circuit breaker**: Per-provider, shared via `Arc<CircuitBreakerRegistry>` in axum app state.
- **Config precedence**: CLI arg > env var (`PRISM_` prefix) > TOML file > built-in defaults. Hardcoded caps (§8.1) are upper bounds that config cannot exceed.

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

## Security Checklist

When modifying API endpoints or adding features, verify:

- [ ] Blocked query types enforced (ANY, AXFR, IXFR)
- [ ] Target IP validation (no RFC 1918, localhost, link-local, CGNAT, multicast)
- [ ] Query limits respected (max 10 record types, max 4 servers)
- [ ] Timeouts enforced (10s per-query, 30s stream)
- [ ] Rate limiting applied with correct cost calculation
- [ ] No PII in logs (no full DNS response content)
- [ ] Security headers present on all responses
- [ ] CORS restricted to same origin
