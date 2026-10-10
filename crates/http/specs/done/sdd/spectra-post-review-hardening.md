# SDD: Spectra Post-Review Hardening

**Status**: Implemented (2026-04-10)
**Original**: specs/sdd/spectra-review.md
**Refined**: 2026-04-10

---

## Overview

Address all High findings and the most impactful Medium findings from the comprehensive 7-lens dev-review of spectra v0.1.0. The goal is to raise the service to production quality across security, correctness, performance, observability, and documentation before the first public release.

---

## Context & Constraints

- **Stack**: Rust (Axum 0.8, edition 2024) + SolidJS 1.9 / Vite. Single binary, frontend embedded via rust-embed.
- **Conventions**: `make ci` = lint + test + frontend build; conventional commits; no `Co-Authored-By`; scoped changes only.
- **Rules**: Apply `logging-rules.md` for telemetry changes; `frontend-rules.md` for all frontend changes; `architecture-rules.md` for health/middleware changes.
- **No breaking API changes**: `InspectResponse` JSON shape is consumed by external clients. Field additions are permitted; removals are not. `AppError` error codes currently used by clients: `INVALID_URL`, `TARGET_BLOCKED`, `INVALID_TARGET`, `RATE_LIMITED`, `TIMEOUT`. Do not add or rename error codes without client coordination.

---

## Architecture

No structural changes to the service topology. Changes are confined to existing modules:

- `src/inspect/request.rs` — SSRF redirect guard, shared client, `http_version` fix
- `src/inspect/security.rs` — Referrer-Policy value table fix
- `src/inspect/cookies.rs` — case-insensitive attribute parsing
- `src/inspect/mod.rs` — `#[tracing::instrument]` on `inspect()`
- `src/state.rs` — add shared `reqwest::Client`, add `http_client` field
- `src/main.rs` — move `security_headers` layer construction to startup
- `src/routes.rs` — meaningful `ready_handler`, histogram instrumentation
- `src/error.rs` — remove `AppError::ConnectionFailed`
- `src/config.rs` — remove `body_read_limit_bytes`, change `log_format` default
- `src/metrics.rs` — new file; register histogram and probe-failure counter
- `.github/workflows/ci.yml` — pin `rustsec/audit-check`, add coverage gate
- `.github/workflows/deploy.yml` — fix `jq` shell injection
- `frontend/src/components/UrlInput.tsx` — responsive layout, aria-label, elapsed timer
- `frontend/src/components/HeadersView.tsx` — `<thead>`
- `frontend/src/components/CorsReport.tsx` — dangerous CORS combo badge
- Various expand/collapse button — `aria-pressed`

---

## Requirements

### Security

**R1.** The system shall re-validate every redirect destination URL through `is_allowed_target` (from `src/input.rs`) before following it. When a redirect destination fails validation, the probe shall abort and set `TaskResult.error = Some("Redirect destination blocked: {url}")`. This is a probe-level error; the route handler still returns HTTP 200 with a partial `InspectResponse` (existing partial-result pattern). No new error type is introduced. The maximum redirect chain depth is 10 hops (the `max_redirects` config value, which defaults to 10). After reaching the limit, the probe shall stop and record the hops captured so far.

**R2.** The deploy workflow (`.github/workflows/deploy.yml`) shall construct the webhook JSON payload using `jq` with `--arg` rather than shell string interpolation of `$TAG`. Exact invocation:
```sh
PAYLOAD=$(jq -n --arg tag "$TAG" '{"ref":$tag}')
```
Replace the current `PAYLOAD="{\"ref\":\"${TAG}\"}"` line with this.

**R3.** The `rustsec/audit-check` step in `.github/workflows/ci.yml` (currently `uses: rustsec/audit-check@v2`) shall be pinned to the commit SHA corresponding to the `v2` tag at implementation time. Add a comment on the same line documenting the version, e.g.:
```yaml
uses: rustsec/audit-check@<SHA> # v2
```

### Performance

**R4.** A single shared `reqwest::Client` shall be created once in `AppState::new()` and stored as `pub http_client: Arc<reqwest::Client>`. This base client shall be built without `.resolve()` host pins and without per-request `Origin` headers; those settings remain per-call in `execute_request`. The `execute_request` function in `src/inspect/request.rs` shall accept a `client: &reqwest::Client` parameter instead of building one internally, and shall construct a new `RequestBuilder` from it per call (which still allows per-call headers). Signature:
```rust
pub async fn execute_request(
    client: &reqwest::Client,
    url: Url,
    resolved_addr: SocketAddr,
    max_redirects: usize,
    timeout: Duration,
    user_agent: &str,
    cors_origin: Option<&str>,
) -> TaskResult
```
The `inspect()` function in `src/inspect/mod.rs` shall receive the client via a new `client: &reqwest::Client` parameter and pass it to each `execute_request` call.

**R5.** The `security_headers` middleware function in `src/security/mod.rs` (or wherever the layer is constructed) shall not reconstruct any static header values on every request. Any `HeaderValue` or `HeaderName` that does not change between requests shall be constructed once at startup (using `std::sync::LazyLock` or `std::sync::OnceLock`) and referenced from the middleware closure.

### Correctness

**R6.** `RedirectHop.http_version` shall be populated with the actual HTTP version of each redirect response. The `format_http_version` function (already present in `src/inspect/request.rs`) shall be used. Within the `Policy::custom` closure, capturing the HTTP version requires switching to a `redirect::Policy` implementation that can access the prior response; if reqwest's `Policy::custom` does not expose the response version, use the `format_http_version` result from the final response for hops where version is unavailable, and document this limitation with a `// TODO` comment. The field type remains `String`; values: `"h0.9"`, `"h1.0"`, `"h1.1"`, `"h2"`, `"h3"` (matching the existing `format_http_version` function output).

**R7.** The `analyze_header_presence` call for `referrer-policy` in `src/inspect/security.rs` shall map values as follows (case-insensitive):
- `Pass`: `no-referrer`, `no-referrer-when-downgrade`, `strict-origin`, `strict-origin-when-cross-origin`, `same-origin`
- `Warn`: `origin`, `origin-when-cross-origin`, `unsafe-url`
- `Fail`: header absent (handled by `analyze_header_presence` wrapper)

Any value not in the Pass or Warn sets shall map to `CheckStatus::Warn` with message `"Unrecognized value: {v}"`.

**R8.** Cookie attribute parsing in `src/inspect/cookies.rs` shall compare all attribute name prefixes case-insensitively. The `secure`, `httponly`, and `samesite` comparisons are already case-insensitive via `to_lowercase()`; no change needed there. Fix the `Path=`, `Domain=`, and `Expires=` prefix strips, which currently use case-sensitive `str::strip_prefix`. Replace with case-insensitive equivalents using the existing `lower` binding (or derive it from `part.to_ascii_lowercase()`).

**R9.** `AppError::ConnectionFailed` shall be deleted from `src/error.rs`. Remove: the variant declaration, its `#[allow(dead_code)]` annotation, its `status_code()` arm (`StatusCode::BAD_GATEWAY`), its `error_code()` arm (`"CONNECTION_FAILED"`), and the `StatusCode::BAD_GATEWAY =>` arm in the `IntoResponse` match block. There are no existing tests for `ConnectionFailed`; no test changes required.

**R10.** `body_read_limit_bytes` shall be removed from:
- `src/config.rs` (`InspectConfig` struct and any `Default` impl)
- `spectra.example.toml`
- `Dockerfile` (if referenced)
- Any documentation or README section that mentions it

### Observability

**R11.** A new file `src/metrics.rs` shall call `metrics::describe_*` at startup to register descriptions for the new metrics. The project uses the `metrics` crate facade (v0.24) with `metrics-exporter-prometheus` as backend — do **not** add a direct `prometheus` crate dependency. Implementation:
```rust
pub fn register_metrics() {
    metrics::describe_histogram!(
        "spectra_inspect_duration_ms",
        metrics::Unit::Milliseconds,
        "Inspection end-to-end duration in milliseconds"
    );
    metrics::describe_counter!(
        "spectra_probe_failures_total",
        "Number of probe failures by probe type"
    );
}
```
Call `crate::metrics::register_metrics()` once during startup in `src/main.rs`. Add `pub mod metrics;` to `src/lib.rs`. At the point of recording, use:
```rust
metrics::histogram!("spectra_inspect_duration_ms", duration_ms as f64);
metrics::counter!("spectra_probe_failures_total", "probe" => "https").increment(1);
```
`AppState` does not need to carry metric handles.

**R12.** After each call to `inspect()` in the route handler (or in `src/inspect/mod.rs` after `tokio::join!`), increment `spectra_probe_failures_total` for each failed probe using `metrics::counter!("spectra_probe_failures_total", "probe" => "https").increment(1)` etc. Label values: `https`, `http_upgrade`, `cors`. A probe is considered failed when its `TaskResult.error` is `Some(_)`. Single label `probe`; no `reason` sublabel.

**R13.** Rename the existing counter `inspect_requests_total` to `spectra_inspect_requests_total` wherever it is defined and referenced. Search `src/` for `inspect_requests_total` and update all occurrences.

**R14.** In `src/config.rs`, change the `Default` impl for `TelemetryConfig` (or its equivalent) so that `log_format` defaults to `"json"`. Update `spectra.example.toml` to reflect `log_format = "json"` as the documented default; add a comment: `# Override with log_format = "text" for local development`.

**R15.** `ready_handler` in `src/routes.rs` already returns `200 {"status":"ready"}` — no body change needed. The only change is to update the handler signature to accept `State(_state): State<AppState>` (note the `_` prefix to satisfy clippy `unused_variables`). This makes the `AppState` dependency explicit for future extension. Because `AppState::new()` is infallible and routes are only registered after `AppState` is constructed, the handler remains unconditionally `200`.

**R16.** The `inspect()` function in `src/inspect/mod.rs` shall be annotated:
```rust
#[tracing::instrument(skip(client, config), fields(url = %url, request_id = tracing::field::Empty))]
pub async fn inspect(
    url: &Url,
    resolved_addr: SocketAddr,
    config: &InspectConfig,
    client: &reqwest::Client,
) -> Result<InspectResult, crate::error::AppError>
```
The `request_id` field shall be filled by the caller (the route handler) via `tracing::Span::current().record("request_id", &request_id)` before calling `inspect()`. URL is not PII at this inspection layer.

### Testing

**R17.** Add route-level integration tests in `src/routes.rs` (in the existing `mod tests` block) using `tower::ServiceExt::oneshot` — the established pattern already used in that file. Do not add `axum_test` as a dependency. Required test cases:
- Valid URL → 200 with `InspectResponse` JSON (mock HTTP server)
- SSRF-blocked URL (`https://10.0.0.1`) → 403 `BLOCKED_TARGET`
- Rate-limited IP → 429 `RATE_LIMITED`
- Malformed URL (e.g. `"not-a-url"`) → 400 `INVALID_URL`
- Enrichment unavailable → 200 with `InspectResponse` where `enrichment.org` is `null`

**R18.** Add unit tests for `inspect()` in `src/inspect/mod.rs` or a submodule. Required test cases (each using a mock TCP server or `tokio_test`):
- All probes succeed → `InspectResult` with no errors
- One probe fails (mock server closes connection) → partial result, other probes populate normally
- Total timeout exceeded → `AppError::Timeout` returned
- HTTP-upgrade detected → `InspectResult.http_upgrade` is `Some` with `redirects_to_https: true`

**R19.** Add unit tests for `validate_target` / `is_allowed_target` in `src/input.rs`. Required addresses (each shall return a blocked/Err result):
- `10.0.0.1` (RFC1918 10/8)
- `172.16.0.1` (RFC1918 172.16/12)
- `172.31.255.255` (RFC1918 172.16/12 upper bound)
- `192.168.0.1` (RFC1918 192.168/16)
- `169.254.1.1` (link-local)
- `100.64.0.1` (CGNAT)

**R20.** Fix the existing test `missing_default_src_is_fail` in `src/inspect/csp.rs` to assert `status == CheckStatus::Fail` (not `Pass` or `Warn`).

**R21.** Add a coverage gate to `.github/workflows/ci.yml`. After the `cargo test` step, add:
```yaml
- name: Coverage
  run: cargo llvm-cov --fail-under-lines 70
```
Add `cargo-llvm-cov` to the CI `cargo install` or use the `taiki-e/install-action` for `cargo-llvm-cov`. The gate must pass before `make ci` is considered green.

### Frontend / UX

**R22.** Add a `@media (max-width: 640px)` CSS rule for `.url-input` (or its container) in the relevant stylesheet under `frontend/src/styles/` so the input form wraps gracefully on narrow viewports. The input and button shall each take full width (100%) at this breakpoint.

**R23.** The clear button in `frontend/src/components/UrlInput.tsx` shall have `aria-label="Clear"` on its JSX element.

**R24.** During an active inspection, `frontend/src/components/UrlInput.tsx` (or the loading state component) shall display elapsed time updated every second. Implementation:
```tsx
const [elapsed, setElapsed] = createSignal(0);
onMount(() => {
  const id = setInterval(() => setElapsed(s => s + 1), 1000);
  onCleanup(() => clearInterval(id));
});
// Display: `${elapsed()}s`
```
Format: integer seconds followed by `"s"`, e.g. `"3s"`. Reset `elapsed` to 0 when a new inspection starts.

**R25.** `frontend/src/components/HeadersView.tsx` shall include a `<thead>` element with `<th>Header</th>` and `<th>Value</th>` column labels.

**R26.** The "Expand all / Collapse all" button (locate in `frontend/src/components/` — search for the expand/collapse toggle) shall carry `aria-pressed={isExpanded() ? "true" : "false"}`. Initial `aria-pressed` value shall be `"false"` (default state: all collapsed).

**R27.** `frontend/src/components/CorsReport.tsx` shall render a `<span class="badge badge--fail">` with text `"Credentials + wildcard"` (or equivalent) when `allows_any_origin === true && allows_credentials === true`.

### Documentation

**R28.** Audit `README.md` quality checks table against the verdict levels actually emitted by `run_checks()` in `src/quality/checks.rs` and `analyze_header_presence()` in `src/inspect/security.rs`. This is a documentation-only fix: if the README table values don't match what the code emits, update the README to match the code. Do not change code to match the README.

**R29.** `Dockerfile` shall reference `spectra.example.toml` in both the `COPY` instruction and the `CMD`/`ENTRYPOINT`, with an operator comment:
```dockerfile
# Operators: copy spectra.example.toml to spectra.toml and edit before deploying
COPY spectra.example.toml /etc/spectra/spectra.example.toml
```

**R30.** `spectra/CLAUDE.md` (not the root `CLAUDE.md`) shall document the `NODE_AUTH_TOKEN` requirement for `npm ci`. Add under the "Build & test" section:
```
cd frontend && npm ci  # needs NODE_AUTH_TOKEN=<GitHub PAT with read:packages> for @netray-info/common-frontend
```

**R31.** Audit `README.md` `make dev` and `make run` descriptions against the actual `Makefile` targets. Update the README descriptions to accurately reflect what each target does. No Makefile changes.

**R32.** Create `CHANGELOG.md` at the repo root with a v0.1.0 entry following Keep a Changelog format. Date: 2026-04-10.

Template:
```markdown
# Changelog

All notable changes to spectra are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0] - 2026-04-10

### Added
- HTTP header inspection and security audit service (HTTP/HTTPS/CORS three-probe analysis)
- CSP directive parsing and scoring
- HSTS, X-Frame-Options, Referrer-Policy, Permissions-Policy, COOP/COEP/CORP checks
- Cookie attribute inspection (Secure, HttpOnly, SameSite)
- CDN detection, caching header analysis, fingerprint leak detection
- Quality verdict engine (Pass/Warn/Fail) with per-check and aggregate scores
- IP enrichment integration via ifconfig-rs
- Prometheus metrics: spectra_inspect_duration_ms, spectra_inspect_requests_total, spectra_probe_failures_total
- OpenAPI 3.1 docs at /docs (Scalar UI)
- SolidJS 1.9 embedded frontend

### Fixed
- SSRF via redirect chain: redirect destinations are now validated before following
- Shell injection in deploy workflow: jq --arg used for JSON payload construction
- Referrer-Policy: risky values (origin, unsafe-url) now produce Warn instead of Pass
- Cookie attribute parsing is now case-insensitive per RFC 6265
- Prometheus counter namespace: all metrics use spectra_ prefix
```

---

## File & Module Structure

Files modified or created by this SDD:

| File | Change |
|------|--------|
| `src/metrics.rs` | New file; histogram and counter registration |
| `src/error.rs` | Remove `ConnectionFailed` variant |
| `src/config.rs` | Remove `body_read_limit_bytes`; default `log_format = "json"` |
| `src/state.rs` | Add `http_client: Arc<reqwest::Client>` field |
| `src/main.rs` | Move `security_headers` layer construction; add `mod metrics` |
| `src/routes.rs` | Update `ready_handler`; add histogram recording; pass client to `inspect()` |
| `src/inspect/mod.rs` | Add `client` param; `#[tracing::instrument]`; increment probe-failure counters |
| `src/inspect/request.rs` | Accept `client: &reqwest::Client`; add SSRF redirect guard; fix `http_version` |
| `src/inspect/security.rs` | Fix Referrer-Policy value table |
| `src/inspect/cookies.rs` | Case-insensitive attribute parsing |
| `spectra.example.toml` | Remove `body_read_limit_bytes`; `log_format = "json"` |
| `Dockerfile` | Reference `spectra.example.toml` with operator comment |
| `.github/workflows/ci.yml` | Pin `rustsec/audit-check`; add coverage step |
| `.github/workflows/deploy.yml` | Fix `jq` shell injection |
| `frontend/src/components/UrlInput.tsx` | Responsive layout; `aria-label`; elapsed timer |
| `frontend/src/components/HeadersView.tsx` | Add `<thead>` |
| `frontend/src/components/CorsReport.tsx` | Dangerous CORS combo badge |
| `frontend/src/components/<expand-collapse>` | `aria-pressed` |
| `frontend/src/styles/*.css` | `@media (max-width: 640px)` for `.url-input` |
| `README.md` | Fix quality table; `make dev`/`make run` descriptions; R30 already in CLAUDE.md |
| `spectra/CLAUDE.md` | Document `NODE_AUTH_TOKEN` |
| `CHANGELOG.md` | New file; v0.1.0 entry |

---

## Data Models

No new top-level types. Changes to existing types:

**`AppState`** (`src/state.rs`):
```rust
pub struct AppState {
    pub config: Arc<Config>,
    pub ip_extractor: Arc<IpExtractor>,
    pub rate_limiter: Arc<RateLimitState>,
    pub enrichment_client: Option<Arc<EnrichmentClient>>,
    pub http_client: Arc<reqwest::Client>,  // NEW
}
```

**`AppError`** (`src/error.rs`): remove `ConnectionFailed` variant.

**`RedirectHop`** (`src/inspect/assembler.rs`): type unchanged (`http_version: String`); values now populated (previously always `""`).

**`InspectConfig`** (`src/config.rs`): remove `body_read_limit_bytes: usize` field.

---

## API Contracts

No changes to existing API contracts. The `InspectResponse` JSON shape is unchanged. The `ready_handler` response body is already `{"status":"ready"}` — no change.

The error response for a redirect-to-blocked-target during probing is surfaced as a probe-level error (stored in `TaskResult.error`) and does not become an `AppError` at the route level — the route still returns 200 with a partial result where the failed probe's fields are populated with the error message. This preserves the existing pattern where probe failures are non-fatal to the overall inspection.

---

## Configuration

Changes to `spectra.example.toml`:

1. Remove `body_read_limit_bytes` line from `[inspect]` section.
2. Change `log_format = "text"` to `log_format = "json"` in `[telemetry]` section, with comment `# Override with log_format = "text" for local development`.

No new config keys added.

---

## Error Handling

| Failure | Trigger | Behaviour | User-visible |
|---------|---------|-----------|-------------|
| SSRF via redirect | `Location` header resolves to blocked IP/range | Probe aborts; `TaskResult.error` set to `"Redirect destination blocked: {url}"`; overall 200 with partial result. No new `AppError` variant. | `redirects` array may be incomplete; probe result shows error message |
| Direct SSRF (existing) | Initial URL resolves to blocked IP | Route returns 403 | `{"error":{"code":"BLOCKED_TARGET","message":"..."}}` |
| Redirect chain depth exceeded | `redirect_limit_reached` flag set | Probe records hops so far; `redirect_limit_reached` field set in response | Visible via `redirect_limit_reached` field |
| Probe failure (non-SSRF) | Connection error, TLS error, etc. | `TaskResult.error` set; `spectra_probe_failures_total{probe}` incremented; overall 200 | Probe result may be empty/partial |
| Total timeout | All three probes exceed `total_timeout_secs` | Route returns 504 | `{"error":{"code":"TIMEOUT","message":"..."}}` |
| Rate limit | Per-IP or per-target limit exceeded | Route returns 429 with `Retry-After` | `{"error":{"code":"RATE_LIMITED","message":"..."}}` |
| Enrichment unavailable | `enrichment_client` is `None` or call fails | Returns 200; `enrichment.org`, `enrichment.ip_type`, `enrichment.threat`, `enrichment.role` are `null` | Partial result with null enrichment fields |

---

## Implementation Phases

### Phase 1 — Security (commit: `fix(security): ssrf redirect guard, workflow hardening`)

1. **SSRF redirect guard** (`src/inspect/request.rs`): In the `Policy::custom` closure, after recording the hop, call `is_allowed_target` on `attempt.url()`. If blocked, set a shared `ssrf_blocked: Arc<Mutex<bool>>` flag and call `attempt.stop()`. After `client.get(...).send().await`, check the flag; if set, populate `TaskResult.error` with `"Redirect destination blocked"`. Import `crate::input::is_allowed_target` (or the equivalent public function from `src/input.rs`). *(R1)*

2. **Deploy workflow jq fix** (`.github/workflows/deploy.yml`): Replace `PAYLOAD="{\"ref\":\"${TAG}\"}"` with `PAYLOAD=$(jq -n --arg tag "$TAG" '{"ref":$tag}')`. Ensure `jq` is available on the GitHub Actions runner (it is by default on `ubuntu-latest`). *(R2)*

3. **Pin rustsec/audit-check** (`.github/workflows/ci.yml`): Look up the commit SHA for the `v2` tag of `rustsec/audit-check` at implementation time. Replace `uses: rustsec/audit-check@v2` with `uses: rustsec/audit-check@<SHA> # v2`. *(R3)*

**Phase complete when**: `make ci` passes; integration test confirms URL-that-redirects-to-192.168.x.x returns probe error; no unpinned action tags in `ci.yml`.

---

### Phase 2 — Performance (commit: `perf(inspect): shared http client, static security headers`)

4. **Shared reqwest::Client** (`src/state.rs`, `src/inspect/request.rs`, `src/inspect/mod.rs`):
   - In `AppState::new()`, build `reqwest::Client` with `danger_accept_invalid_certs(true)` and store as `http_client: Arc<reqwest::Client>`.
   - Update `execute_request` signature to accept `client: &reqwest::Client` as first parameter; remove the internal `reqwest::Client::builder()...build()` block.
   - Update `inspect()` signature to accept `client: &reqwest::Client`; pass it to all three `execute_request` calls.
   - Update all call sites in `src/routes.rs` to pass `state.http_client.as_ref()`. *(R4)*

5. **Static security headers** (`src/security/mod.rs` or wherever `security_headers` middleware is defined): Move any `HeaderValue::from_static(...)` or `HeaderValue::try_from(...)` calls that produce constant values into `std::sync::LazyLock` statics at module level. The middleware closure shall only reference these statics, not construct new `HeaderValue`s. *(R5)*

**Phase complete when**: `make ci` passes; `AppState` has one `http_client` field; no `ClientBuilder` calls inside `execute_request`.

---

### Phase 3 — Correctness (commit: `fix(inspect): referrer-policy, cookies, redirect version, dead code`)

6. **RedirectHop http_version** (`src/inspect/request.rs`): The `Policy::custom` closure does not receive the response object, so the HTTP version of the redirect response cannot be determined inside the closure. Populate `http_version` for the final response using `format_http_version(response.version())` (already done). For redirect hops, set `http_version` to the final response's version as a best-effort approximation, and add `// TODO: reqwest Policy::custom does not expose per-hop response version` comment on the field assignment inside the closure. *(R6)*

7. **Referrer-Policy value table** (`src/inspect/security.rs`): Replace the existing `referrer_policy` closure with:
   ```rust
   analyze_header_presence(headers, "referrer-policy", |v| {
       let pass_values = [
           "no-referrer",
           "no-referrer-when-downgrade",
           "strict-origin",
           "strict-origin-when-cross-origin",
           "same-origin",
       ];
       let warn_values = ["origin", "origin-when-cross-origin", "unsafe-url"];
       if pass_values.iter().any(|s| v.eq_ignore_ascii_case(s)) {
           (CheckStatus::Pass, None)
       } else if warn_values.iter().any(|s| v.eq_ignore_ascii_case(s)) {
           (CheckStatus::Warn, Some(format!("Permissive policy: {v}")))
       } else {
           (CheckStatus::Warn, Some(format!("Unrecognized value: {v}")))
       }
   })
   ``` *(R7)*

8. **Cookie case-insensitive parsing** (`src/inspect/cookies.rs`): The `secure`/`httponly`/`samesite` comparisons already use `to_lowercase()` — no change there. Fix the `Path=`/`Domain=`/`Expires=` prefix strips which currently use case-sensitive `str::strip_prefix`. Derive a lowercase binding from each attribute part and use it for all prefix checks. *(R8)*

9. **Remove AppError::ConnectionFailed** (`src/error.rs`): Delete the variant declaration, its `#[allow(dead_code)]` annotation, its `status_code()` arm, its `error_code()` arm, and the `StatusCode::BAD_GATEWAY =>` arm in the `IntoResponse` match block. No tests reference this variant; no test changes required. *(R9)*

10. **Remove body_read_limit_bytes** (`src/config.rs`, `spectra.example.toml`, `Dockerfile` if present): Remove the field from the `InspectConfig` struct, its `Default` impl, and the example TOML. If `Dockerfile` references it via env var (`SPECTRA_INSPECT__BODY_READ_LIMIT_BYTES`), remove that line. *(R10)*

**Phase complete when**: `make ci` passes; `origin` Referrer-Policy returns `Warn` in test; `HTTPONLY` cookie attribute is parsed; `ConnectionFailed` does not appear in `src/error.rs`.

---

### Phase 4 — Observability (commit: `feat(metrics): inspect histogram, probe counters, ready handler`)

11. **Create `src/metrics.rs`**: Add `register_metrics()` function using `metrics::describe_*` calls (see Requirements R11). Add `pub mod metrics;` to `src/lib.rs`. Call `crate::metrics::register_metrics()` in `src/main.rs` during startup. *(R11)*

12. **Increment probe-failure counter** (`src/inspect/mod.rs`): After `tokio::join!` returns, check `https.error.is_some()`, `cors.error.is_some()`, and `upgrade.as_ref().and_then(|r| r.error.as_ref()).is_some()`. For each truthy case, call `metrics::counter!("spectra_probe_failures_total", "probe" => "https").increment(1)` etc. *(R12)*

13. **Rename counter** (`src/routes.rs` or wherever `inspect_requests_total` is defined/used): Rename to `spectra_inspect_requests_total`. Run `grep -r "inspect_requests_total" src/` to find all occurrences. *(R13)*

14. **Default log_format = "json"** (`src/config.rs`): In the `Default` impl for `TelemetryConfig`, change `log_format` default from `"text"` to `"json"`. Update `spectra.example.toml`. *(R14)*

15. **Update ready_handler signature** (`src/routes.rs`): Add `State(_state): State<AppState>` parameter to `ready_handler`. The response body is already `Json(ReadyResponse { status: "ready" })` — no body change. No change to `ReadyResponse` type. *(R15)*

16. **Instrument inspect()** (`src/inspect/mod.rs`): Add `#[tracing::instrument(skip(client, config), fields(url = %url, request_id = tracing::field::Empty))]` to `inspect()`. In the route handler in `src/routes.rs`, before calling `inspect()`, record the request_id: `tracing::Span::current().record("request_id", &request_id.as_str())` (adjust based on how `request_id` is accessed in that handler). *(R16)*

17. **Record histogram** (`src/routes.rs`): Wrap the `inspect()` call with a timer. After assembly, call `metrics::histogram!("spectra_inspect_duration_ms", duration_ms as f64)`. *(R11)*

**Phase complete when**: `make ci` passes; `GET /metrics` returns `spectra_inspect_duration_ms_bucket` and `spectra_probe_failures_total`; log output is JSON.

---

### Phase 5 — Testing (three separate commits)

**Commit A**: `test(routes): integration tests for inspect endpoint` *(R17)*
18. Add route integration tests in the existing `mod tests` block in `src/routes.rs`. Use `tower::ServiceExt::oneshot` (existing pattern; `tower` is already a dev-dependency). Mock external HTTP servers with `tokio::net::TcpListener` — same pattern as the existing `redirect_hops_are_captured` test in `src/inspect/request.rs`. Test cases listed in R17.

**Commit B**: `test(inspect): orchestrator unit tests` *(R18)*
19. Add unit tests for `inspect()` in `src/inspect/mod.rs` or `tests/inspect_tests.rs`. Use `tokio::net::TcpListener`-based mock servers for probe targets. Timeout test: set `InspectConfig.total_timeout_secs = 1` and have the mock server sleep for 2 seconds.

**Commit C**: `test(input): ssrf range coverage + csp fix` *(R19, R20)*
20. Add `validate_target` unit tests in `src/input.rs` for all addresses listed in R19.
21. Fix `missing_default_src_is_fail` in `src/inspect/csp.rs` to assert `status == CheckStatus::Fail`.

**Commit D**: `ci: add llvm-cov coverage gate at 70%` *(R21)*
22. Add `cargo-llvm-cov` to CI. In `.github/workflows/ci.yml`, after the `cargo test` step in the `test` job, add:
    ```yaml
    - name: Install cargo-llvm-cov
      uses: taiki-e/install-action@v2
      with:
        tool: cargo-llvm-cov
    - name: Coverage gate
      run: cargo llvm-cov --fail-under-lines 70
    ```

**Phase complete when**: `make ci` passes with coverage gate green; all four commits are independently mergeable.

---

### Phase 6 — Frontend / UX (commit: `feat(frontend): responsive input, aria attributes, elapsed timer, cors badge`)

23. **Responsive URL input** (`frontend/src/styles/*.css`): Add to the stylesheet that defines `.url-input` (find by grepping `frontend/src/styles/`):
    ```css
    @media (max-width: 640px) {
      .url-input { flex-direction: column; }
      .url-input input,
      .url-input button { width: 100%; }
    }
    ``` *(R22)*

24. **Clear button aria-label** (`frontend/src/components/UrlInput.tsx`): Find the clear button JSX element and add `aria-label="Clear"`. *(R23)*

25. **Elapsed timer** (`frontend/src/components/UrlInput.tsx` or loading state component):
    ```tsx
    const [elapsed, setElapsed] = createSignal(0);
    // Reset on new inspection start:
    // setElapsed(0);
    // Inside loading JSX:
    createEffect(() => {
      if (!isLoading()) return;
      setElapsed(0);
      const id = setInterval(() => setElapsed(s => s + 1), 1000);
      onCleanup(() => clearInterval(id));
    });
    // Display: <span>{elapsed()}s</span>
    ``` *(R24)*

26. **HeadersView thead** (`frontend/src/components/HeadersView.tsx`): Wrap existing header row in `<thead><tr><th>Header</th><th>Value</th></tr></thead>` and wrap body rows in `<tbody>`. *(R25)*

27. **aria-pressed on expand/collapse** (locate by searching `frontend/src/` for `expand` or `collapse` toggle button): Add `aria-pressed={isExpanded() ? "true" : "false"}` to the toggle button. Set initial state of the `isExpanded` signal to `false`. *(R26)*

28. **CorsReport dangerous combo badge** (`frontend/src/components/CorsReport.tsx`): Find where `allows_any_origin` and `allows_credentials` are rendered. Add:
    ```tsx
    {data.allows_any_origin && data.allows_credentials && (
      <span class="badge badge--fail">Credentials + wildcard</span>
    )}
    ``` *(R27)*

**Phase complete when**: `make ci` passes; aria attributes present in DOM; elapsed timer visible during inspection; narrow viewport wraps correctly.

---

### Phase 7 — Documentation (commit: `docs(spectra): changelog, readme audit, dockerfile, claude-md`)

29. Audit README quality checks table against `run_checks()` output. Fix discrepancies (doc only). *(R28)*
30. Update `Dockerfile` to reference `spectra.example.toml` with operator comment. *(R29)*
31. Add `NODE_AUTH_TOKEN` note to `spectra/CLAUDE.md`. *(R30)*
32. Audit and fix `make dev` / `make run` descriptions in README. *(R31)*
33. Create `CHANGELOG.md` with v0.1.0 entry. *(R32)*
34. Move this SDD to `specs/done/sdd/spectra-post-review-hardening.md`.

**Phase complete when**: `docker build` succeeds from a clean clone; a new developer can follow README + CLAUDE.md without errors.

---

## Test Scenarios

### SSRF via redirect blocked (R1)
```
GIVEN a mock HTTP server at 127.0.0.1:<port> that returns 302 Location: http://192.168.1.1/admin
WHEN POST /api/inspect is called with that server's URL
THEN the overall response status is 200
AND InspectResponse contains a probe TaskResult.error containing "Redirect destination blocked"
AND no outbound TCP connection is made to 192.168.1.1
```

### Duration histogram populated (R11)
```
GIVEN an inspection completes successfully
WHEN GET /metrics is called
THEN the response body contains a line matching spectra_inspect_duration_ms_bucket
AND at least one bucket has count > 0
AND spectra_inspect_requests_total is incremented
```

### Readiness probe (R15)
```
GIVEN the service is accepting connections (AppState is constructed)
WHEN GET /ready is called
THEN response is 200 {"status":"ready"}
```

### Referrer-Policy risky value warns (R7)
```
GIVEN an HTTP response with header Referrer-Policy: origin
WHEN analyze_security_headers is called with those headers
THEN security_report.referrer_policy.status == CheckStatus::Warn
```

### Referrer-Policy unknown value warns (R7)
```
GIVEN an HTTP response with header Referrer-Policy: some-unknown-value
WHEN analyze_security_headers is called with those headers
THEN security_report.referrer_policy.status == CheckStatus::Warn
AND referrer_policy.message contains "Unrecognized value"
```

### SSRF blocked route (R17)
```
GIVEN POST /api/inspect {"url":"https://10.0.0.1"}
THEN response status is 403
AND body is {"error":{"code":"TARGET_BLOCKED","message":"<any>"}}
```

### Enrichment unavailable (R17)
```
GIVEN AppState.enrichment_client is None
WHEN POST /api/inspect is called with a valid reachable URL
THEN response status is 200
AND InspectResponse.enrichment.org is null
AND InspectResponse.enrichment.ip_type is null
```

### Probe failure counter incremented (R12)
```
GIVEN the HTTPS probe fails (mock server returns error)
WHEN GET /metrics is called after the inspection
THEN spectra_probe_failures_total{probe="https"} >= 1
```

### validate_target blocks CGNAT (R19)
```
GIVEN validate_target is called with 100.64.0.1
THEN it returns an Err (blocked)
```

### CorsReport dangerous combo (R27)
```
GIVEN allows_any_origin == true AND allows_credentials == true
WHEN CorsReport renders
THEN DOM contains element with class badge--fail
```

---

## Decision Log

| Decision | Alternatives considered | Rationale |
|----------|------------------------|-----------|
| Shared `reqwest::Client` in `AppState` | Per-request client (current) | Single shared client is idiomatic reqwest; enables connection pool reuse across probes and requests |
| SSRF redirect: abort with probe error (200 overall) | Abort with 403 at route level; follow and redact | Route-level 403 would require detecting redirect-SSRF before calling inspect(); probe-level error preserves the existing partial-result pattern where individual probes can fail without failing the whole request |
| Default `log_format = "json"` | Keep text default | A public service scraped by Loki requires JSON; text should be the local-dev exception |
| Remove `body_read_limit_bytes` | Keep with dead-code annotation | YAGNI — dead config adds confusion; re-add when body sniffing is implemented |
| 70% coverage gate | 80% (too strict for first pass); no gate | 70% is achievable by covering the core pipeline; establishes the gate without blocking the release |
| Remove `AppError::ConnectionFailed` | Wire to reqwest connect errors | No current caller; adding a new user-visible error code (`CONNECTION_FAILED`) risks breaking clients expecting the existing set; per YAGNI, remove now and re-add when needed |
| Readiness check: structural only | TCP connect to enrichment host | TCP connect adds startup latency and blocks readiness if enrichment is down (which is best-effort); AppState is always fully constructed before routes are registered, so the check is meaningful at that layer |
| `jq -n --arg tag "$TAG" '{"ref":$tag}'` | `printf '{"ref":"%s"}' "$TAG"` | jq is the canonical safe way to produce JSON; avoids all quoting and injection edge cases |
| Referrer-Policy: `no-referrer-when-downgrade` = Pass | Warn (it leaks referrer on HTTP) | The value is widely deployed as a browser default; treating it as Pass avoids false positives for the majority of sites. Implementors may revisit in a future hardening pass. |

---

## Open Decisions

None. All ambiguous findings resolved above.

---

## Out of Scope

- Body sniffing / response content analysis
- Frontend test framework setup (vitest) — separate initiative
- Rate-limiter eviction / TTL for `KeyedLimiter` memory growth — operational hardening, future pass
- HTTP/3 / QUIC support detection
- Historical scan tracking or result caching
- TCP-connect-based readiness probe (deferred; see Decision Log)
