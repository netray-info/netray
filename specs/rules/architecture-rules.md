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
- Documented in OpenAPI under a "Probes" tag where utoipa is used.
