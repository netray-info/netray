# CLAUDE.md -- spectra

## What this is

HTTP header inspector and security audit service (`http.netray.info`). Fourth pillar in the netray suite: IP -> DNS -> TLS -> HTTP.

Given a URL, fires three concurrent requests (HTTPS chain, HTTP port-80 upgrade probe, CORS probe with evil origin), parses every response header category, and returns a structured JSON report with per-check quality verdicts.

## Architecture

Axum 0.8 service with embedded SolidJS 1.9 frontend. Follows the same patterns as tlsight/prism.

- `src/input.rs` -- URL normalization + SSRF validation (delegates to netray-common target_policy)
- `src/inspect/` -- Header analysis modules (security, csp, cors, cookies, caching, fingerprint)
- `src/inspect/request.rs` -- reqwest client with custom redirect policy for hop capture
- `src/quality/` -- Per-check scoring engine; CheckStatus ordering: Pass < Skip < Warn < Fail
- `src/security/` -- IP extraction and rate limiting (delegates to netray-common)
- `src/routes.rs` -- API handlers, health/ready endpoints

## Config

TOML file `spectra.toml` + env overrides with `SPECTRA_` prefix (`__` for nesting).

## Key conventions

- Health endpoints: `GET /health`, `GET /ready` (root level per architecture-rules)
- API: `GET/POST /api/inspect`, `GET /api/config`, `GET /docs` (Scalar UI)
- Rate limiting: per-IP (10/min, burst 5) + per-target (30/min, burst 10)

## Development

```sh
cargo build                          # build backend
cargo test                           # run all tests
cargo clippy -- -D warnings          # lint
cd frontend && npm install           # install frontend deps (needs NODE_AUTH_TOKEN)
cd frontend && npm run dev           # Vite dev server on :5175
cd frontend && npm run build         # production build into dist/
```

## Specs

- SDD: [`specs/done/sdd/http-inspector.md`](../specs/done/sdd/http-inspector.md)
- Apply [frontend-rules](../specs/rules/frontend-rules.md) when modifying `frontend/`
- Apply [logging-rules](../specs/rules/logging-rules.md) when modifying tracing/telemetry
- Apply [architecture-rules](../specs/rules/architecture-rules.md) for health probes and middleware
