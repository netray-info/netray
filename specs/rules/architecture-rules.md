# Architecture Rules

Cross-service conventions for the netray.info suite.

---

## Probe Endpoints

Health and readiness probes are infrastructure concerns, not API resources.
They live at root level, not under `/api/`.

| Path | Purpose | Response |
|------|---------|----------|
| `GET /health` | Liveness probe | `{"status":"ok"}` · always 200 |
| `GET /ready` | Readiness probe | `{"status":"ready"}` · 200 or 503 |

Both endpoints must be:
- Exempt from rate limiting (mount outside rate-limit middleware layers).
- Served with `Cache-Control: no-cache`.
- Documented in OpenAPI under a "Probes" tag.

---

## OpenAPI

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
- CSP for `/docs` must be relaxed to allow the Scalar UI's inline scripts (existing pattern: per-route CSP override).
