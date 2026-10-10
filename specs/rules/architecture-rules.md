# Architecture Rules

Cross-service conventions for the netray.info suite. Apply when adding or modifying any HTTP service, shared library, or cross-service protocol.

---

## Configuration

- Format: TOML file + environment variable overrides.
- Per-tool env prefix: `PRISM_`, `TLSIGHT_`, `LENS_`; `BEACON__` with a double underscore. A module converted to the V2 engine uses `NETRAY_<P>_` instead (http: `NETRAY_HTTP_`, `NETRAY_HTTP_CONFIG`; ip: `NETRAY_IP_`, `NETRAY_IP_CONFIG`) and refuses its old prefix at load via `netray_common::config::refuse_legacy_prefix`. `<TOOL>_CONFIG` names the file and is never a key.
- Nested sections use double-underscore: `PRISM_LIMITS__PER_IP_PER_MINUTE=60`. Every config struct denies unknown keys, so an unknown variable under the prefix fails startup (enforced: `tests/repo/test_config_strict.sh`, `netray <sub> --check-config`).
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

Every response, error responses and CORS preflights included, carries what Traefik's `secure-headers`, `csp-tool-spa` and `cors-public-api` middlewares add in production (parity, SDD M19), set by `netray_common::security_headers` and `netray_common::cors`:

- `Strict-Transport-Security: max-age=31536000; includeSubDomains; preload`; ifconfig-rs sets two years through `SecurityHeadersConfig.hsts`, never in its own code.
- `Content-Security-Policy`: exactly `csp-tool-spa`; only `/docs` is relaxed, and every CDN script the docs page loads must match a `script-src` source under CSP source-matching rules (a source ending in `/` is a path prefix).
- `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=()`, `Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Resource-Policy: cross-origin`; no `Server`, no `X-Powered-By`.
- CORS: `Access-Control-Allow-Origin: *`, methods `GET, POST, OPTIONS`, headers `Content-Type, Accept`, max-age 600, no credentials.
- `netray site` keeps its own set (`secure-headers` plus `csp-netray-web`, CORP `same-origin`).

Enforced: `tests/repo/test_header_parity.sh` (gate), `tests/acceptance/smoke/security-headers.spec.ts` (`just acceptance-local`, `just acceptance`).

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
