# Post-Review Fix Plan

Generated from the 2026-03-07 dev review across 7 lenses.
Findings are grouped into phases by severity and logical dependency.

---

## Phase 1 — Critical & High: Correctness and Security

These fixes address the most impactful bugs, security gaps, and correctness issues.
None require architectural changes; all are self-contained and independently mergeable.

### 1.1 Parallelize `check.rs` DNS lookups

**Lens**: Engineering / Architecture
**Files**: `src/api/check.rs`

Replace the sequential `for rt in CHECK_RECORD_TYPES` loop with `FuturesUnordered`,
matching the pattern already used in `query.rs`. The check endpoint currently serializes
15 DNS lookups, making it up to 15× slower than necessary and holding rate-limit stream
slots much longer under load.

### 1.2 Extract shared constants and deduplicate structs

**Lens**: Engineering
**Files**: `src/api/query.rs`, `src/api/check.rs`, `src/api/trace.rs`

- Define `STREAM_TIMEOUT_SECS: u64 = 30` in a single shared location (e.g. `src/api/mod.rs`)
  and reference it from all three endpoint modules.
- Consolidate the identical `BatchEvent` struct, defined separately in `query.rs` and
  `check.rs`, into a single definition in `src/api/mod.rs` or a new `src/api/events.rs`.

### 1.3 Block IPv6 ULA ranges in query policy

**Lens**: Security
**Files**: `src/security/query_policy.rs`

Add `fc00::/7` (IPv6 unique-local) to `is_allowed_target`. The current check only covers
IPv4 private ranges; with `allow_arbitrary_servers = true` an attacker can direct queries
to internal IPv6 infrastructure.

### 1.4 Apply private-IP blocklist to resolved glue addresses

**Lens**: Security / Architecture
**Files**: `src/dns_trace.rs`

`resolve_missing_glue` calls `tokio::net::lookup_host` (system resolver) and uses the
resulting addresses directly, bypassing the query policy private-IP check, rate limiting,
and circuit breaking. After resolving glue, pass each resulting `IpAddr` through
`is_allowed_target` and discard any that fail.

### 1.5 Fix CORS `null`-origin misconfiguration

**Lens**: Security / Engineering
**Files**: `src/security/mod.rs`

`AllowOrigin::exact("null")` permits the `null` origin (sandboxed iframes, `file://`
pages). The intent is same-origin only. Replace with `AllowOrigin::any()` guarded by
`allow_headers` / `allow_methods` restrictions, or remove the `allow_origin` call
entirely so the CORS layer only emits headers on actual cross-origin requests. Update
the comment to accurately describe the behaviour.

### 1.6 Secure the metrics endpoint in container deployments

**Lens**: Security / Architecture
**Files**: `src/main.rs`, `Dockerfile`

- Add a note in the Dockerfile that port 9090 should NOT be published externally.
- Document in README/config that `metrics_bind` should remain on a non-routable
  interface in production.
- Optionally add a Bearer token check via config if public exposure is ever needed.

### 1.7 Add warning log for silently-dropped trusted proxy entries

**Lens**: Engineering / Security
**Files**: `src/security/ip_extract.rs`, `src/config.rs`

`IpExtractor::new` silently discards invalid `trusted_proxies` entries. The comment
claims they are "logged at warn level during config validation" but no such log exists.
Add `tracing::warn!` for each discarded entry so misconfiguration is observable.

---

## Phase 2 — High: Observability

These fixes give operators the signals needed to understand production behaviour.
They are grouped separately because they touch the logging/metrics infrastructure
and may require a small refactor to request ID propagation.

### 2.1 Unify the two request IDs

**Lens**: Observability
**Files**: `src/main.rs`, `src/api/query.rs`, `src/api/check.rs`, `src/api/trace.rs`

`request_id_middleware` generates a UUID for the `X-Request-Id` response header;
each SSE handler generates a second, independent UUID for the `request_id` field in
events. These cannot be correlated in logs.

Fix: generate the UUID once in `request_id_middleware`, store it in request extensions
(`req.extensions_mut().insert(RequestId(id))`), and extract it in each handler instead
of calling `Uuid::now_v7()` a second time.

### 2.2 Switch to JSON structured logging

**Lens**: Observability
**Files**: `src/main.rs`

Replace `tracing_subscriber::fmt()` with the JSON format layer
(`.json()` or `tracing_subscriber::fmt::format::Json`). JSON logs are required for
field-level querying in any production log aggregator (Loki, CloudWatch, Datadog).

### 2.3 Add `info`-level per-request logging and log HTTP 500 errors

**Lens**: Observability
**Files**: `src/api/query.rs`, `src/api/check.rs`, `src/api/trace.rs`, `src/error.rs`

- Emit a `tracing::info!` event on query completion (domain, record types, status,
  duration_ms) — currently everything is at `debug`.
- In `ApiError::IntoResponse`, add `tracing::error!` for variants that map to HTTP 500
  (`ResolverError`, `Internal`) so server-side errors are never silent.

### 2.4 Add `prism_http_requests_total` counter

**Lens**: Observability
**Files**: `src/main.rs` or a new middleware

Add a Tower middleware (or configure `TraceLayer` callbacks) to emit a
`prism_http_requests_total{method, path, status}` counter. This is the minimum signal
needed to build an error-rate SLO in Prometheus.

### 2.5 Distinguish liveness from readiness in health endpoints

**Lens**: Observability
**Files**: `src/api/meta.rs`, `src/api/mod.rs`

Add a `GET /api/ready` endpoint that checks circuit breaker state
(`CircuitBreakerRegistry` has no open breakers) and returns 503 if degraded. Keep
`GET /api/health` as the cheap liveness check. This enables proper Kubernetes probe
separation.

---

## Phase 3 — High/Medium: Test Coverage

Adding tests is a prerequisite for the refactors in Phase 4. These should be added
before making larger structural changes.

### 3.1 Add HTTP handler integration tests

**Lens**: Testing
**Files**: `src/api/query.rs`, `src/api/check.rs`, `src/api/trace.rs`, `src/api/meta.rs`

Use axum's `TestClient` (from `axum::test`) with mocked DNS resolvers to cover:
- Parse errors → correct HTTP 400 with `ApiError` JSON body
- Policy violations → HTTP 422
- Rate limiting → HTTP 429 with `Retry-After` header
- SSE stream produces at least one `batch` event and a `done` event
- `GET /api/health` returns 200
- `GET /api/servers` and `GET /api/record-types` return non-empty arrays

### 3.2 Add `config::validate()` tests

**Lens**: Testing
**Files**: `src/config.rs`

Test that:
- Hard-cap values clamp rather than reject
- Zero values for limits are rejected
- Invalid `bind` addresses are rejected at validation time

### 3.3 Add `error.rs` `IntoResponse` tests

**Lens**: Testing
**Files**: `src/error.rs`

Test that each `ApiError` variant maps to the correct HTTP status code, that the JSON
body shape matches `{"error": {"code": "...", "message": "..."}}`, and that
`RateLimited` includes the `Retry-After` header.

### 3.4 Wire up frontend tests in CI

**Lens**: Testing
**Files**: `.github/workflows/ci.yml`, `frontend/package.json`

- Add a `"test": "vitest run"` script to `frontend/package.json`.
- Add an `npm test` step to the CI workflow after `npm run build`.
- Add unit tests for `src/lib/tokenizer.ts` (token classification, edge cases).

### 3.5 Fix policy test isolation

**Lens**: Testing
**Files**: `src/security/query_policy.rs`

Replace `Box::leak` + real env var loading in `make_policy()` with a
`Config::default()` or a test-specific config constructor that does not read the
environment, so policy tests are not affected by `PRISM_*` env vars on the runner.

---

## Phase 4 — Medium: Engineering Cleanup

These are internal quality improvements with no user-visible behaviour change.
Safe to batch into a single cleanup PR after Phase 1–3 land.

### 4.1 Consolidate `PostServerSpec` and `ErrorBody`/`ErrorResponse` duplication

**Files**: `src/api/query.rs`, `src/error.rs`

- `PostServerSpec` is a single-variant enum (`Named(String)`) with no discriminant
  benefit. Replace with `String` at the use sites in `PostQueryRequest` and
  `CheckRequest`, removing the irrefutable destructures.
- Merge `ErrorResponse`/`ErrorInfo` (OpenAPI schema) and `ErrorBody`/`ErrorDetail`
  (actual serialization) into a single pub type used for both purposes, eliminating the
  schema-vs-wire drift risk.

### 4.2 Extract shared fan-out helper

**Files**: `src/api/query.rs`, `src/api/check.rs`

After 1.1 parallellizes check, both endpoints will use `FuturesUnordered` with the same
circuit-breaker pre-check and result-merge pattern. Extract the per-record-type fan-out
into a shared helper in `src/api/mod.rs` or `src/api/dns_exec.rs`.

### 4.3 Fix `IpExtractor` CIDR-range support or document the limitation

**Files**: `src/security/ip_extract.rs`, `src/config.rs`

The `trusted_proxies` config only accepts individual IP addresses, not CIDR ranges
(common in Kubernetes where pod IPs change). Either add CIDR support using the `ipnet`
crate, or add a startup validation error (not silent skip) when a CIDR-like string is
provided.

### 4.4 Fix hardcoded `MAX_RECORD_TYPES` in parser

**Files**: `src/parser.rs`, `src/config.rs`

`parser.rs` enforces `MAX_RECORD_TYPES = 10` independently of `config.limits.max_record_types`.
Pass the configured limit into the parser so the two enforcement points are consistent.
If the hardcoded cap is intentional as an absolute upper bound, document it clearly and
add a startup assertion that `max_record_types <= MAX_RECORD_TYPES`.

### 4.5 Minor cleanups

**Files**: various

- `record_to_dns_record` in `dns_trace.rs:677` always returns `Some(…)` — change return
  type to the concrete type and update call sites.
- Remove `#[allow(dead_code)]` on `ApiError::BlockedQueryType` and either wire it up
  in the query handler or delete the variant.
- Remove `ApiError::Internal` `#[allow(dead_code)]` once handler tests confirm it is
  used.
- Replace `expect("UUID is valid header value")` in `request_id_middleware` with
  `unwrap_or_else` — `src/main.rs:114`.
- Fix `tokio::signal::unix::signal` double-registration for SIGTERM: share a single
  shutdown channel between the main server and metrics server — `src/main.rs:97,210`.

---

## Phase 5 — Medium/High: UX and Accessibility

User-facing improvements. Can be done in parallel with Phase 4.

### 5.1 Accessible loading states

**Files**: `frontend/src/components/ResultsTable.tsx`, `frontend/src/components/TraceView.tsx`

- Add `role="status"` and `aria-live="polite"` to the loading spinner wrapper.
- Replace the three empty `.trace-pending-dot` spans with a single element carrying
  `aria-label="Loading trace"`.

### 5.2 Fix help modal accessibility

**Files**: `frontend/src/App.tsx`

- Add `role="dialog"`, `aria-modal="true"`, `aria-labelledby` to the modal element.
- Trap focus inside the modal on open (move focus to the first focusable child; restore
  on close).
- Add `<thead>` to the reference table.

### 5.3 Disable submit during in-flight request

**Files**: `frontend/src/App.tsx`, `frontend/src/components/QueryInput.tsx`

Pass `disabled={status() === 'loading'}` to the submit button and add a visual
indicator (e.g. spinner on the button) so users know a request is in flight. Prevent
the `r` keyboard shortcut from firing a second request while streaming.

### 5.4 ARIA roles for interactive elements

**Files**: `frontend/src/components/ResultsTable.tsx`, `frontend/src/components/TraceView.tsx`, `frontend/src/App.tsx`

- Add `role="tab"` / `role="tablist"` / `aria-selected` to the tab bar.
- Add `tabIndex={0}` and `role="row"` (or `button`) to expandable table rows; wire
  Enter/Space key handlers.
- Add `tabIndex={0}`, `role="button"`, and `aria-label` to `HopCard`.
- Add `aria-controls` to the record-group-header collapse button.

### 5.5 Fix dark theme colour contrast

**Files**: `frontend/src/styles/global.css`

- `--text-muted: #606060` on `--bg-primary: #1a1a2e` is ~2.5:1 — fails WCAG AA.
  Lighten to at least `#888` (≥4.5:1) or change usage sites to `--text-secondary`.
- Verify small-caps lint badge text (`--success` green on dark background) meets the
  4.5:1 threshold at the rendered font size.

### 5.6 Welcome card discoverability

**Files**: `frontend/src/App.tsx`

Add a short inline hint below each welcome card example button (e.g. "fills the query
bar — press Enter to run") so the two-step fill→submit interaction is discoverable
without a hover tooltip.

---

## Phase 6 — Medium/Low: Documentation Sync

The SDD has accumulated significant staleness. These fixes are documentation-only and
carry no code risk.

### 6.1 Update SDD to reflect current implementation

**File**: `docs/sdd.md`

- §3.2: Add `check.rs`, `trace.rs`, `dns_trace.rs`, `circuit_breaker.rs`,
  `LintTab.tsx`, `TraceView.tsx` to the directory layout.
- §5.1 `done` event: update field names to match `DoneEvent`
  (`total_queries`, `duration_ms`, `warnings`, `transport`, `dnssec`).
- §5.2 check `done` event: align field names with `CheckDoneEvent`.
- §5.3 trace `hop` event: change flat schema to nested `{"hop": {…}}`.
- §5.6 error schema: remove the `"details"` field (not present in `ErrorDetail`).
- §7.1: Replace `Config::from_env_and_args()` with `Config::load()`.
- §8.2: Update rate limit table to match config defaults
  (120/min burst 40 per-IP, 1000/min global).
- §9: Correct config auto-discovery documentation — no default path probing exists.
- §14: Mark Phase 4 as complete; update status header.

### 6.2 Fix README API examples

**File**: `README.md`

- `POST /api/query` example: replace `{"query": "..."}` with the actual struct fields
  (`domain`, `record_types`, `servers`, `transport`, `dnssec`).
- `POST /api/check` servers example: replace `{"predefined": "cloudflare"}` with
  plain string `"cloudflare"`.
- Add `[trace]` config section to the configuration reference table.
- Add a section or line mentioning the `/docs` API reference UI.

### 6.3 Update CLAUDE.md

**File**: `CLAUDE.md`

- Add `LintTab.tsx` and `TraceView.tsx` to the frontend component tree.
- Clarify that check cost is `16 × server_count` while trace cost is flat `16`.
- Add a note about `/docs` and `/api-docs/openapi.json`.

---

## Summary Table

| Phase | Focus                          | Severity        | Effort  |
|-------|--------------------------------|-----------------|---------|
| 1     | Correctness & security fixes   | Critical / High | Medium  |
| 2     | Observability                  | High / Medium   | Medium  |
| 3     | Test coverage                  | Critical / High | High    |
| 4     | Engineering cleanup            | Medium / Low    | Low     |
| 5     | UX & accessibility             | High / Medium   | Medium  |
| 6     | Documentation sync             | High / Medium   | Low     |

Phases 1, 2, and 3 should be treated as blocking for any production deployment.
Phases 4–6 are quality improvements that can be scheduled alongside feature work.
