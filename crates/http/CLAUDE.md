# CLAUDE.md -- spectra

## What this is

HTTP header inspector and security audit service (`http.netray.info`). Fourth pillar in the netray suite: IP -> DNS -> TLS -> HTTP.

Given a URL, fires three concurrent requests (HTTPS chain, HTTP port-80 upgrade probe, CORS probe with evil origin), parses every response header category, and returns a structured JSON report with per-check quality verdicts.

## Architecture

Axum 0.8 service with embedded SolidJS 1.9 frontend. Follows the same patterns as tlsight/prism.

- `src/input.rs` -- URL normalization + SSRF validation (delegates to netray-common target_policy)
- `src/inspect/` -- Header analysis modules (security, csp, cors, cookies, caching, fingerprint)
- `src/inspect/headers.rs` -- Raw header dump utility
- `src/inspect/mod.rs` -- Inspection orchestrator; runs three concurrent probes
- `src/inspect/request.rs` -- reqwest client with custom redirect policy for hop capture
- `src/quality/` -- Per-check scoring engine; CheckStatus ordering: Pass < Skip < Warn < Fail
- `src/security/` -- IP extraction and rate limiting (delegates to netray-common)
- `src/routes.rs` -- API handlers, health/ready endpoints

## Config

TOML file (default: `spectra.dev.toml` for local dev) + env overrides as `SPECTRA__<SECTION>__<KEY>` (double underscore after the prefix too). Set `SPECTRA_CONFIG` to override the config file path. Every config struct is `deny_unknown_fields`: an unknown section or key, in the file or the env, fails the load. `netray http --check-config <path>` validates a file, including the values startup rejects (zero rate limits), and exits 0 (`config ok: <path>`) or 1 with the error.

## Key conventions

- Health endpoints: `GET /health`, `GET /ready` (root level per architecture-rules)
- API: `GET/POST /api/inspect`, `GET /api/config`, `GET /api/meta`, `GET /docs` (Scalar UI)
- Rate limiting: per-IP (10/min, burst 5) + per-target (30/min, burst 10)
- Rate limit fires before DNS resolution to avoid unnecessary lookups for throttled clients

## Development

### Setup

`spectra.dev.toml` is committed; copy `spectra.example.toml` for a config of your own.

### Running locally

```sh
netray http crates/spectra/spectra.dev.toml  # starts the service (after `just build`)
SPECTRA_CONFIG=my.toml netray http           # use a custom config file path
```

### Build & test

The verbs live in the root `justfile` (see the root `README.md`); run them from the repository root.

```sh
just adlc-setup                      # once: npm workspaces + frontend builds
just adlc-verify                     # the gate: fmt-check, clippy, tests, offline
cargo test -p spectra                # this crate's tests
npm run dev -w spectra-frontend      # Vite dev server on :5175
```

## Specs

- Apply [frontend-rules](../../specs/rules/frontend-rules.md) when modifying `frontend/`
- Apply [logging-rules](../../specs/rules/logging-rules.md) when modifying tracing/telemetry
- Apply [architecture-rules](../../specs/rules/architecture-rules.md) for health probes and middleware
