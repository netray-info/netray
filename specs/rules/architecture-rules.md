# Architecture Rules

Cross-service conventions for the netray.info suite. Apply when adding or modifying any HTTP service, shared library, or cross-service protocol.

---

## Configuration

- Format: TOML file + environment variable overrides.
- Per-tool env prefix: `IFCONFIG_`, `PRISM_`, `TLSIGHT_`, `SPECTRA_`, `BEACON_`, `LENS_`.
- Nested sections use double-underscore: `PRISM_TELEMETRY__LEVEL=debug`.
- Every config value exposed through env must also have a TOML counterpart; do not add env-only knobs.

---

## Error Format

All error responses share one shape:

```json
{"error": {"code": "string", "message": "string"}}
```

- `code` is a short machine-readable identifier (snake or kebab case, stable across versions).
- `message` is a human-readable description; do not include stack traces, PII, or internal paths.
- On rate-limit responses (HTTP 429), include a `Retry-After` header.

---

## Rate Limiting

- Use `governor` with the GCRA algorithm.
- Costs are per-endpoint and weighted by expected load; document the cost in the route annotation.
- Probe endpoints (`/health`, `/ready`) and docs endpoints (`/api-docs/openapi.json`, `/docs`) are exempt.
- Reject with HTTP 429 + the standard error shape + `Retry-After`.

---

## Security Headers

Every response (including error responses) must carry:

- `Content-Security-Policy` — strict; loosened only on `/docs` to permit Scalar's inline scripts (per-route override).
- `Strict-Transport-Security` — `max-age=63072000; includeSubDomains; preload`.
- `X-Frame-Options: DENY`.
- `X-Content-Type-Options: nosniff`.

---

## Request IDs

- Emit `X-Request-Id` on every response.
- New services: use UUID v7 (sortable, matches prism/tlsight/spectra/beacon).
- Legacy exception: ifconfig-rs uses a hex counter; do not change without a migration plan.
- Honour an incoming `X-Request-Id` if supplied by an upstream service (trusted inner network only).

---

## Probe Endpoints

Health and readiness probes are infrastructure concerns, not API resources. They live at root level, not under `/api/`.

| Path | Purpose | Response |
|------|---------|----------|
| `GET /health` | Liveness probe | `{"status":"ok"}` · always 200 |
| `GET /ready` | Readiness probe | `{"status":"ready"}` · 200 or 503 |

Both endpoints must be:

- Exempt from rate limiting (mount outside rate-limit middleware layers).
- Served with `Cache-Control: no-cache`.
- Documented in OpenAPI under a "Probes" tag.

---

## OpenAPI & API Docs

Every tool must expose a machine-readable OpenAPI 3.1 spec and an interactive UI.

| Path | Purpose |
|------|---------|
| `GET /api-docs/openapi.json` | OpenAPI 3.1 spec (JSON) |
| `GET /docs` | Scalar interactive UI |

Requirements:

- Use **utoipa 5** with `utoipa-axum` for route annotations and `utoipa-scalar` for the UI.
- Annotate every public API endpoint with `#[utoipa::path]`. Probe endpoints go under tag `"Probes"`.
- All request and response types must derive `ToSchema` (or `IntoParams` for query/path params).
- Error types shared across endpoints must be defined once and `$ref`-ed via utoipa's component system — do not duplicate inline.
- The spec must be served without authentication and exempt from rate limiting.
- CSP for `/docs` is relaxed via per-route override to allow Scalar's inline scripts.

---

## Frontend Embedding

- SolidJS 1.9 SPA per tool, built with Vite.
- Embedded into the Rust binary via `rust-embed`; served from the root path.
- Build artifacts live under `frontend/dist/`; never commit them.
- See `frontend-rules.md` for SolidJS, CSS, and accessibility standards.

---

## Observability

- Prometheus `/metrics` on the admin port (9090 by default, separate from the public port).
- OpenTelemetry is optional per tool; configured through `[telemetry]` in the TOML config.
- See `logging-rules.md` for tracing init, log filters, and field conventions.

---

## Cargo Features

- Shared crates (`netray-common`, and any dependency two services use) must not switch behaviour on a Cargo feature: in the one `netray` binary features unify, so one service's feature changes every other service. Expose the choice at runtime instead (e.g. `EnrichmentMode`, beacon's UDP/TCP resolver filter). unenforced
