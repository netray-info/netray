# SDD: Spectra Post-Review Hardening

Status: Ready for Implementation
Original: specs/sdd/spectra-review.md
Refined: 2026-04-09

---

## Overview

Address HIGH and MEDIUM findings from the April 2026 dev-review across seven lenses (engineering, security, testing, UX, architecture, observability, docs). All changes are additive or internal — no breaking API changes. Work is divided into nine independent, committable phases.

---

## Context & Constraints

- Rust 2024 / Axum 0.8 backend, SolidJS 1.9 frontend, single binary via rust-embed.
- netray-common 0.5.4 from crates.io; path deps are forbidden in CI.
- Config: TOML + env overrides, `SPECTRA_` prefix, `__` for nested sections.
- Frontend: Vite, TypeScript, CSS classes (no inline styles), BEM-ish naming per `specs/rules/frontend-rules.md`.
- Metrics crate: `metrics` 0.24 + `metrics-exporter-prometheus` 0.18 (already in `Cargo.toml`).
- The `metrics` crate is used via its macro API (`metrics::counter!`). The Prometheus exporter is installed by `netray_common::server::serve_metrics`.

---

## Architecture

No new modules. All changes are modifications to existing files. The enrichment client (`EnrichmentClient` from netray-common) manages its own connection pool and is unchanged. `execute_request` continues to build a per-request `reqwest::Client` — this is intentional and necessary because `.resolve()` and `Policy::custom()` cannot be set post-construction.

---

## Requirements

### Backend

R1. The system shall register `GET /api/meta` returning a JSON object with shape:
```json
{ "version": "<CARGO_PKG_VERSION>", "site_name": "spectra", "ecosystem": { "ip_base_url": "...", "dns_base_url": "...", "tls_base_url": "...", "http_base_url": "...", "lens_base_url": "..." } }
```
All `ecosystem` fields are optional strings read from `[meta]` config section keys `ip_base_url`, `dns_base_url`, `tls_base_url`, `http_base_url`, `lens_base_url`. The `version` field is always present, sourced from `env!("CARGO_PKG_VERSION")`. The `site_name` field is always `"spectra"`.

R2. The system shall move the rate-limit check (`state.rate_limiter.check(...)`) in `do_inspect` (`src/routes.rs`) to execute before `crate::input::validate_target`, eliminating DNS lookups for rate-limited clients. Current order: parse → resolve → rate-limit → inspect. New order: parse → rate-limit → resolve → inspect.

R4. The system shall log a tracing `warn` event when the enrichment lookup returns `Err`, replacing the silent `.unwrap_or_default()` in `do_inspect` (`src/routes.rs`). Log field: `error = %e`, message: `"enrichment lookup failed"`.

R5. The system shall record `client_ip` into the active tracing span after extraction in `do_inspect` (`src/routes.rs`) using `tracing::Span::current().record("client_ip", client_ip.to_string())`. The span field name is `client_ip`.

R6. The system shall emit a `metrics::counter!("inspect_requests_total", "outcome" => outcome)` call at the end of `do_inspect`, where `outcome` is one of the following string values:
- `"success"` — inspection completed and a response was assembled.
- `"rate_limited"` — `AppError::RateLimited` returned.
- `"blocked"` — `AppError::BlockedTarget` returned.
- `"timeout"` — `AppError::Timeout` returned.
- `"error"` — any other `AppError` variant returned.

The counter is incremented unconditionally on every call path. Place the increment at the return point in `do_inspect` by matching on the `Result` before returning it.

R7. The system shall log `TaskResult.error` at `warn` level when a probe result contains `Some(error)`, in `inspect::inspect` (`src/inspect/mod.rs`) after the `tokio::join!`. Log once per failing probe with field `error = %e` and a message indicating which probe failed (e.g., `"https probe failed"`, `"http_upgrade probe failed"`, `"cors probe failed"`).

R8. `InspectResponse` (`src/inspect/assembler.rs`) shall derive `ToSchema`. `SecurityReport`, `CspReport`, and `CookieEntry` shall derive `ToSchema`. `InspectResponse` shall be added to the `#[openapi(components(schemas(...)))]` list in `src/routes.rs`. The inspect handler `utoipa::path` annotations shall specify `body = InspectResponse` in their 200 responses.

R9. `danger_accept_invalid_certs(true)` in `src/inspect/request.rs` shall have an inline comment: `// Intentional: inspecting sites with broken or self-signed certs is a core feature.`

### Testing

R10. SSRF tests in `src/input.rs` that currently call `validate_target` with IP-literal URLs (`https://127.0.0.1`, `https://192.168.1.1`) do not make real DNS calls and are correct as-is. Add tests for IPv6 SSRF ranges using IP-literal URLs: `https://[::1]`, `https://[fc00::1]`, `https://[fe80::1%2525eth0]` — use the pattern `Url::parse("https://[::1]").unwrap()` to avoid DNS. Each shall return `Err(AppError::BlockedTarget(_))`.

R11. `src/inspect/request.rs` shall have at least one unit test for redirect hop capture. Use a `tokio::net::TcpListener` bound to `127.0.0.1:0` in the test, serving a minimal HTTP 301 response (raw bytes), and verify `TaskResult.redirects.len() >= 1`. Do not add `mockito`, `wiremock`, or any new dev-dependency — neither is in `Cargo.toml`.

R12. `src/quality/checks.rs` shall have tests for:
  - Insecure cookie warning: a response with one cookie where `secure = false` produces a check with `name = "cookie_secure"` and `status = CheckStatus::Warn`.
  - Deprecated headers warning: a response with `deprecated_headers = vec!["x-powered-by".into()]` produces a check with `name = "deprecated_headers"` and `status = CheckStatus::Warn`.
  - Redirect limit branch: a response with `redirect_limit_reached = Some(10)` produces a check with `name = "redirect_limit"` and `status = CheckStatus::Warn`.

R13. `src/security/rate_limit.rs` shall have a test for the per-target limiter where two different client IPs target the same hostname, exhaust the per-target burst, and the subsequent call returns `Err`.

### Frontend / UX

R14. Each of the 9 `section-card__header` buttons in `frontend/src/App.tsx` shall have an `aria-controls` attribute whose value matches the `id` of the corresponding `section-card__body` div. The 9 sections and their `id` / button pairings:

| Section signal | Button line (approx) | `aria-controls` value | Body `id` |
|---|---|---|---|
| `openQuality` | ~314 | `"section-quality-body"` | `"section-quality-body"` |
| `openRedirects` | ~385 | `"section-redirects-body"` | `"section-redirects-body"` |
| `openSecurity` | ~408 | `"section-security-body"` | `"section-security-body"` |
| `openCsp` | ~441 | `"section-csp-body"` | `"section-csp-body"` |
| `openCors` | ~466 | `"section-cors-body"` | `"section-cors-body"` |
| `openCookies` | ~485 | `"section-cookies-body"` | `"section-cookies-body"` |
| `openCaching` | ~504 | `"section-caching-body"` | `"section-caching-body"` |
| `openFingerprint` | ~520 | `"section-fingerprint-body"` | `"section-fingerprint-body"` |
| `openHeaders` | ~541 | `"section-headers-body"` | `"section-headers-body"` |

R15. Remove `tabIndex={-1}` from the clear button in `frontend/src/components/UrlInput.tsx` (line 51). The button shall be keyboard-reachable.

R16. `frontend/src/styles/global.css` shall have a `@media (max-width: 640px)` block containing:
```css
@media (max-width: 640px) {
  table { overflow-x: auto; display: block; }
  .check-list { min-width: 0; }
}
```

R17. COOP, COEP, and CORP rows in `frontend/src/components/SecurityAudit.tsx` shall have explanation text. Use the following hardcoded strings if not already wired from `qualityChecks`:
- COOP: `"Cross-Origin Opener Policy isolates the browsing context from cross-origin documents, preventing cross-origin attacks via window references."`
- COEP: `"Cross-Origin Embedder Policy prevents documents from loading cross-origin resources unless they grant explicit permission, enabling isolation features."`
- CORP: `"Cross-Origin Resource Policy prevents other origins from reading this resource's content, protecting against speculative execution attacks."`

R18. `frontend/src/components/CachingView.tsx`, `CorsReport.tsx`, `FingerprintView.tsx`, and `RedirectChain.tsx` shall replace all `style={{...}}` objects with CSS classes defined in `frontend/src/styles/global.css`. New class names to add:

| Component | Current inline style purpose | New class |
|---|---|---|
| `CachingView` | `font-size: 0.8125rem; margin-bottom: 0.5rem` on `<p class="mono">` | `.cache-value` |
| `CachingView` | `list-style: none; padding: 0; margin: 0 0 0.75rem 0; font-size: 0.875rem` on directive list | `.cache-directive-list` |
| `CachingView` | `list-style: none; padding: 0; margin: 0; font-size: 0.875rem` on vary list | `.cache-vary-list` |
| `CachingView` | `margin-top: 0.75rem` wrapper div | `.cache-cdn-section` |
| `CachingView` | CDN label div (uppercase, muted, letter-spacing) | `.section-label` (reuse if exists, else add) |
| `CachingView` | CDN description `<p style="font-size: 0.875rem">` | `.cache-cdn-desc` |
| `CachingView` | indicators `<p class="mono" style="font-size: 0.8125rem; color: var(--text-muted)">` | `.cache-indicators` |
| `CorsReport` | `font-size: 0.875rem; margin-bottom: 0.5rem` on message `<p>` | `.cors-message` |
| `CorsReport` | `list-style: none; padding: 0; margin: 0; font-size: 0.8125rem` on flags list | `.cors-flags-list` |
| `FingerprintView` | `font-size: 0.875rem` on server `<p>` | `.fingerprint-server` |
| `FingerprintView` | `font-size: 0.875rem; margin-top: 0.5rem; color: var(--warn)` on leakage `<p>` | `.fingerprint-leak-warn` |
| `RedirectChain` | `margin-top: 1rem` wrapper div | `.redirect-upgrade-section` |
| `RedirectChain` | upgrade label div (uppercase, muted, letter-spacing) | `.section-label` (reuse if exists, else add) |
| `RedirectChain` | upgrade message `<p style="font-size: 0.875rem; margin-bottom: 0.5rem">` | `.redirect-upgrade-message` |

If a `.section-label` class already exists in `global.css` with equivalent styles, reuse it instead of adding a duplicate.

R19. `frontend/src/components/ExportButtons.tsx`: the `downloadJson` function shall wrap `new URL(r.final_url).hostname` in a try/catch. On error, fall back to `r.url` (the original URL string) as the filename base. The existing `copyMarkdown` function already has a try/catch for this; mirror that pattern.

R20. `frontend/src/components/ExportButtons.tsx`: the `copyMarkdown` function shall include a CORS section and a redirect chain section. Insert after the Cookies section and before the footer line:

```
## CORS
- Allows any origin: <yes|no>
- Reflects origin: <yes|no>
- Allows credentials: <yes|no>
- Status: <status>
- <message>

## Redirects
<for each hop: `- [<status>] <url> → <location>`>
```

Emit the CORS section unconditionally (it is always present). Emit the Redirects section only if `r.redirects.length > 0`.

### Docs / Config

R21. `CLAUDE.md` development section shall use `npm ci` instead of `npm install`.

R22. `CLAUDE.md` shall document that `make run` starts the service from zero (reads `spectra.toml` or falls back to defaults) and that `SPECTRA_CONFIG` overrides the config file path. Add under a "Running locally" sub-heading in the Development section.

R23. `CLAUDE.md` module inventory for `src/inspect/` shall include `headers.rs` and `mod.rs` (add alongside existing entries; `redirects.rs` is deleted in R26).

R24. `spectra.example.toml` shall have inline comments explaining:
- `trusted_proxies`: `# CIDR list of trusted reverse proxies (e.g. ["10.0.0.0/8"]). Required when running behind a load balancer. Omit if directly exposed.`
- `enrichment.ip_url`: `# URL of the ifconfig-rs enrichment service. Omit or leave blank to disable IP enrichment.`
- `body_read_limit_bytes`: `# Reserved for future body sniffing. Has no effect in the current version.`
- `[meta]` section: add the commented-out block from the Configuration section above (all keys commented out, with a header comment explaining their purpose for cross-navigation links).

R25. `spectra.toml` shall be deleted from the repository. Operators are expected to create their own config file; the repo shall not ship a default that could be deployed unmodified.
R25a. `spectra.dev.toml` shall be created as the local development config. It shall be listed in `.gitignore` (entry: `spectra.dev.toml`). It shall contain real local values suitable for `cargo run` (e.g. `[enrichment]\nip_url = "http://localhost:3001"`). If a `spectra.dev.toml` already exists, do not overwrite it. The file shall begin with: `# Local development config — do not commit`.

### Engineering Cleanup

R26. `src/inspect/redirects.rs` (placeholder file, contains only comments or is empty) shall be deleted. Its `pub mod redirects;` declaration in `src/inspect/mod.rs` shall be removed.

R27. `EnrichmentData` in `src/inspect/mod.rs` shall replace its manual `Default` impl with `#[derive(Default)]`. The current manual impl returns all `None` fields, which is what `#[derive(Default)]` produces for `Option<T>` fields.

R28. `use crate::inspect::assembler::*` in `src/routes.rs` shall be replaced with explicit imports. Audit the file for which names from `assembler` are actually used and import only those.

---

## File & Module Structure

Files modified (no new files created):

```
src/
  routes.rs         — R2 (rate-limit order), R1 (GET /api/meta), R4 (enrichment warn), R5 (span client_ip), R6 (metrics), R8 (OpenAPI), R28 (explicit imports)
  config.rs         — R1 (MetaConfig struct + Config.meta field)
  inspect/
    mod.rs          — R7 (log probe errors), R27 (derive Default), R26 (remove redirects mod)
    request.rs      — R9 (inline comment), R11 (redirect test)
    assembler.rs    — R8 (ToSchema derives)
    redirects.rs    — DELETE (R26)
  input.rs          — R10 (IPv6 SSRF tests)
  quality/
    checks.rs       — R12 (new tests)
  security/
    rate_limit.rs   — R13 (per-target test)
frontend/src/
  App.tsx           — R14 (aria-controls + body ids)
  components/
    UrlInput.tsx    — R15 (remove tabIndex=-1)
    SecurityAudit.tsx — R17 (COOP/COEP/CORP explanation text)
    CachingView.tsx — R18 (inline styles → CSS classes)
    CorsReport.tsx  — R18 (inline styles → CSS classes)
    FingerprintView.tsx — R18 (inline styles → CSS classes)
    RedirectChain.tsx — R18 (inline styles → CSS classes)
    ExportButtons.tsx — R19 (URL parse safety), R20 (CORS + redirects in markdown)
  styles/
    global.css      — R16 (responsive breakpoint), R18 (new CSS classes)
CLAUDE.md           — R21, R22, R23
spectra.example.toml — R24
spectra.toml        — DELETE (R25)
spectra.dev.toml    — NEW, gitignored (R25a)
.gitignore          — add spectra.dev.toml entry (R25a)
```

---

## Data Models

### MetaResponse (new, in `src/routes.rs`)

```rust
#[derive(Debug, Serialize, ToSchema)]
pub struct MetaResponse {
    pub version: &'static str,
    pub site_name: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecosystem: Option<EcosystemLinks>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EcosystemLinks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens_base_url: Option<String>,
}
```

### MetaConfig (new section in `src/config.rs`)

```rust
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetaConfig {
    #[serde(default)]
    pub ip_base_url: Option<String>,
    #[serde(default)]
    pub dns_base_url: Option<String>,
    #[serde(default)]
    pub tls_base_url: Option<String>,
    #[serde(default)]
    pub http_base_url: Option<String>,
    #[serde(default)]
    pub lens_base_url: Option<String>,
}
```

Add `pub meta: MetaConfig` to `Config` with `#[serde(default)]`. Config key: `[meta]`. Env override: `SPECTRA_META__IP_BASE_URL`, etc.

---

## API Contracts

### GET /api/meta (new)

```
GET /api/meta
200 OK
Content-Type: application/json

{
  "version": "0.4.0",
  "site_name": "spectra",
  "ecosystem": {
    "ip_base_url": "https://ip.netray.info",
    "dns_base_url": "https://dns.netray.info",
    "tls_base_url": "https://tls.netray.info",
    "http_base_url": "https://http.netray.info",
    "lens_base_url": "https://lens.netray.info"
  }
}
```

`ecosystem` is absent if no `[meta]` section is configured. Individual keys within `ecosystem` are absent if their config value is `None`. The handler never returns an error; it always returns 200.

Register in `api_router` alongside `/api/config`. Add `meta_handler` to `#[openapi(paths(...))]` and `MetaResponse`, `EcosystemLinks` to `components(schemas(...))`.

---

## Configuration

### New `[meta]` section in `spectra.example.toml`

```toml
# [meta]
# Base URLs for suite cross-navigation links served to the frontend.
# All keys are optional. Omit the section entirely to disable ecosystem links.
# ip_base_url = "https://ip.netray.info"
# dns_base_url = "https://dns.netray.info"
# tls_base_url = "https://tls.netray.info"
# http_base_url = "https://http.netray.info"
# lens_base_url = "https://lens.netray.info"
```

Env overrides: `SPECTRA_META__IP_BASE_URL`, `SPECTRA_META__DNS_BASE_URL`, etc.

---

## Error Handling

| Failure | Trigger | Behaviour | User-visible |
|---|---|---|---|
| Enrichment lookup fails | `EnrichmentClient::lookup` returns `Err` | Log `warn!(error = %e, "enrichment lookup failed")`; continue with `EnrichmentData::default()` | None — result is returned without enrichment data |
| Probe returns `error: Some(...)` | `TaskResult.error` is `Some` after `tokio::join!` | Log `warn!(error = %e, "<probe_name> probe failed")`; response is assembled from partial data | None — result is returned with empty headers for that probe |
| Rate limit exceeded | Per-IP or per-target GCRA quota exhausted | Return `AppError::RateLimited`; increment `inspect_requests_total{outcome="rate_limited"}` | 429 with `Retry-After` header |
| SSRF block | `validate_target` returns `BlockedTarget` | Return `AppError::BlockedTarget`; increment `inspect_requests_total{outcome="blocked"}` | 403 |
| Total timeout | `tokio::time::timeout` fires | Return `AppError::Timeout`; increment `inspect_requests_total{outcome="timeout"}` | 504 |
| Invalid URL | `parse_url` fails | Return `AppError::InvalidUrl`; increment `inspect_requests_total{outcome="error"}` | 400 |
| `GET /api/meta` | Never fails | Always 200 with at least `{"version":"...","site_name":"spectra"}` | 200 |
| `downloadJson` URL parse error | `new URL(final_url)` throws | Catch exception; use `r.url` as filename base | Download proceeds with fallback filename |

---

## Implementation Phases

### Phase 1 — Rate-limit ordering + observability logging
**Files**: `src/routes.rs`, `src/inspect/mod.rs`

1. Reorder `do_inspect` in `src/routes.rs`: move `state.rate_limiter.check(client_ip, hostname)` to execute after `parse_url` but before `validate_target` (R2). The `hostname` extraction from the parsed URL must happen before the rate-limit call; this is already possible from the parsed `Url`.
2. Record `client_ip` into the tracing span: `tracing::Span::current().record("client_ip", tracing::field::display(client_ip));` after the `let client_ip = ...` line (R5).
3. Replace `.unwrap_or_default()` on enrichment lookup with explicit match: on `Err(e)` log `warn!(error = %e, "enrichment lookup failed")` then use `EnrichmentData::default()` (R4).
4. In `inspect::inspect` (`src/inspect/mod.rs`), after `tokio::join!`, check each `TaskResult.error` and log at warn level (R7):
   ```rust
   if let Some(ref e) = https.error { tracing::warn!(error = %e, "https probe failed"); }
   if let Some(ref e) = upgrade_result.as_ref().and_then(|r| r.error.as_ref()) { tracing::warn!(error = %e, "http_upgrade probe failed"); }
   if let Some(ref e) = cors.error { tracing::warn!(error = %e, "cors probe failed"); }
   ```

**Complete when**: `cargo test` passes; rate-limit fires before DNS in `do_inspect`; enrichment failure logs a `WARN` line; probe errors log at `WARN`.

---

### Phase 2 — Metrics counter
**Files**: `src/routes.rs`

1. Add `inspect_requests_total` counter increments at the return point of `do_inspect`. Wrap the result match:
   ```rust
   let result = assemble_and_return(...);
   let outcome = match &result {
       Ok(_) => "success",
       Err(AppError::RateLimited { .. }) => "rate_limited",
       Err(AppError::BlockedTarget(_)) => "blocked",
       Err(AppError::Timeout(_)) => "timeout",
       Err(_) => "error",
   };
   metrics::counter!("inspect_requests_total", "outcome" => outcome).increment(1);
   result
   ```
   The `metrics` crate is already in `Cargo.toml`. No new dependency needed (R6).

**Complete when**: `inspect_requests_total{outcome="success"}` increments on a successful call visible in Prometheus scrape on `:9090/metrics`.

---

### Phase 3 — GET /api/meta
**Files**: `src/config.rs`, `src/routes.rs`

1. Add `MetaConfig` struct to `src/config.rs` (see Data Models). Add `pub meta: MetaConfig` to `Config` with `#[serde(default)]` (R1).
2. Add `MetaResponse` and `EcosystemLinks` structs to `src/routes.rs` with `#[derive(Serialize, ToSchema)]` (R1).
3. Add `meta_handler` function:
   ```rust
   async fn meta_handler(State(state): State<AppState>) -> Json<MetaResponse> { ... }
   ```
   Build `EcosystemLinks` from `state.config.meta`. If all fields are `None`, set `ecosystem: None`; otherwise `Some(EcosystemLinks { ... })`.
4. Register `GET /api/meta` in `api_router`. Add `meta_handler` to `#[openapi(paths(...))]` and add `MetaResponse`, `EcosystemLinks` to `components(schemas(...))` (R1).
5. Add a test: `GET /api/meta` returns 200 with `body["version"].is_string()`.

**Complete when**: `GET /api/meta` returns 200 JSON with `version` matching `CARGO_PKG_VERSION`; test passes.

---

### Phase 4 — OpenAPI schema completeness
**Files**: `src/inspect/assembler.rs`, `src/routes.rs`

1. Add `#[derive(ToSchema)]` to `InspectResponse`, `SecurityReport`, `CspReport`, `CookieEntry` in `src/inspect/assembler.rs` (R8). These types already have `Serialize`/`Deserialize`; only `ToSchema` is missing.
2. Add `InspectResponse`, `SecurityReport`, `CspReport`, `CookieEntry` to `#[openapi(components(schemas(...)))]` in `src/routes.rs` (R8).
3. Update `inspect_post_handler` and `inspect_get_handler` utoipa annotations: add `(status = 200, description = "Inspection result", body = InspectResponse)` (R8).
4. Add inline comment to `danger_accept_invalid_certs(true)` in `src/inspect/request.rs` (R9).

**Complete when**: `GET /api-docs/openapi.json` includes `InspectResponse`, `SecurityReport`, `CspReport`, `CookieEntry` in `components.schemas`; `cargo clippy -- -D warnings` passes.

---

### Phase 5 — Testing gaps
**Files**: `src/input.rs`, `src/inspect/request.rs`, `src/quality/checks.rs`, `src/security/rate_limit.rs`

1. Add IPv6 SSRF tests to `src/input.rs` (R10):
   ```rust
   #[tokio::test]
   async fn validate_blocks_ipv6_loopback() {
       let url = Url::parse("https://[::1]").unwrap();
       assert!(matches!(validate_target(&url).await, Err(AppError::BlockedTarget(_))));
   }
   // Repeat for fc00::1 and fe80::1 (use percent-encoded form if needed)
   ```
2. Add redirect capture test to `src/inspect/request.rs` (R11). Check `Cargo.toml` for `wiremock` or `mockito`; if absent, spawn a minimal TCP listener in the test that returns `HTTP/1.1 301 Moved\r\nLocation: https://example.com\r\n\r\n`, call `execute_request`, assert `result.redirects.len() >= 1`. Do not add a new test dependency unless one of `wiremock`/`mockito` is already present.
3. Add three quality check tests to `src/quality/checks.rs` (R12):
   - `insecure_cookie_produces_warn`: set one cookie with `secure = false`; assert check `cookie_secure` is `Warn`.
   - `deprecated_header_produces_warn`: set `deprecated_headers = vec!["x-powered-by".into()]`; assert check `deprecated_headers` is `Warn`.
   - `redirect_limit_produces_warn`: set `redirect_limit_reached = Some(10)`; assert check `redirect_limit` is `Warn`.
4. Add per-target rate limiter test to `src/security/rate_limit.rs` (R13):
   ```rust
   #[test]
   fn per_target_exhausted_blocks_different_ip() {
       let state = RateLimitState::new(&test_config()); // burst=10
       let ip1: IpAddr = "198.51.100.10".parse().unwrap();
       let ip2: IpAddr = "198.51.100.11".parse().unwrap();
       for _ in 0..10 {
           let _ = state.check(ip1, "target.example.com");
       }
       // ip2 is a fresh IP but same target — per-target limit is shared
       assert!(state.check(ip2, "target.example.com").is_err());
   }
   ```

**Complete when**: `cargo test` passes with all new tests; no network calls in unit tests.

---

### Phase 6 — Frontend accessibility and responsive CSS
**Files**: `frontend/src/App.tsx`, `frontend/src/components/UrlInput.tsx`, `frontend/src/components/SecurityAudit.tsx`, `frontend/src/styles/global.css`

1. Add `aria-controls` attributes and matching `id` attributes per R14 table. For each section button, add `aria-controls="section-<name>-body"`; for each `section-card__body` div, add `id="section-<name>-body"`.
2. Remove `tabIndex={-1}` from the clear button in `UrlInput.tsx` (R15, line 51).
3. Add responsive CSS block to `global.css` (R16).
4. Add COOP/COEP/CORP explanation strings to `SecurityAudit.tsx` (R17). If the component renders explanation text conditionally from a prop or `qualityChecks` lookup, wire the hardcoded strings as fallback for these three headers. If the component does not yet have an explanation mechanism, add a static tooltip or `<details>` element with the text inline.

**Complete when**: keyboard Tab reaches the clear button; all 9 section buttons have `aria-controls`; tables scroll on narrow viewport; COOP/COEP/CORP rows show explanation text.

---

### Phase 7 — Frontend style consistency and export
**Files**: `frontend/src/components/{CachingView,CorsReport,FingerprintView,RedirectChain}.tsx`, `frontend/src/components/ExportButtons.tsx`, `frontend/src/styles/global.css`

1. Add new CSS classes to `global.css` per R18 table.
2. Replace all `style={{...}}` in `CachingView.tsx`, `CorsReport.tsx`, `FingerprintView.tsx`, `RedirectChain.tsx` with the new classes (R18).
3. Add try/catch to `downloadJson` in `ExportButtons.tsx` (R19).
4. Add CORS section and redirects section to `copyMarkdown` in `ExportButtons.tsx` (R20).

**Complete when**: no `style={{` in the four components; markdown copy includes CORS and redirects sections; `npm run build` succeeds.

---

### Phase 8 — Docs and config
**Files**: `CLAUDE.md`, `spectra.example.toml`, `spectra.toml`, `spectra.dev.toml`, `.gitignore`

1. Replace `npm install` with `npm ci` in `CLAUDE.md` development section (R21).
2. Add "Running locally" sub-section to `CLAUDE.md` development section documenting `make run` and the `SPECTRA_CONFIG` env var (R22). Also add `SPECTRA_CONFIG` to CLAUDE.md key conventions if not present.
3. Update `CLAUDE.md` module inventory for `src/inspect/` to include `headers.rs` and `mod.rs`; remove `redirects.rs` (R23).
4. Add inline comments to `spectra.example.toml` for `trusted_proxies`, `enrichment.ip_url`, `body_read_limit_bytes` (R24). Add commented `[meta]` section.
5. Delete `spectra.toml` from the repository (R25).
6. Create `spectra.dev.toml` from `spectra.example.toml` with local dev values. Add `spectra.dev.toml` to `.gitignore` (R25a).
7. Update `CLAUDE.md` development section to reference `spectra.dev.toml` as the local config file (e.g. `cp spectra.example.toml spectra.dev.toml` as the setup step).

**Complete when**: `spectra.toml` is gone from the repo; `spectra.dev.toml` exists and is gitignored; `spectra.example.toml` is fully annotated; `CLAUDE.md` describes the `spectra.dev.toml` workflow.

---

### Phase 9 — Engineering cleanup
**Files**: `src/inspect/redirects.rs`, `src/inspect/mod.rs`, `src/routes.rs`

1. Delete `src/inspect/redirects.rs` (R26).
2. Remove `pub mod redirects;` from `src/inspect/mod.rs` (R26).
3. Replace `#[derive(Default)]` on `EnrichmentData` in `src/inspect/mod.rs` — remove the manual `impl Default` and add `#[derive(Default)]` to the struct (R27).
4. Replace `use crate::inspect::assembler::*` in `src/routes.rs` with explicit imports. Read the file and list all names actually used from `assembler` before making the change (R28).

**Complete when**: `cargo build` and `cargo clippy -- -D warnings` pass clean; `src/inspect/redirects.rs` no longer exists.

---

## Test Scenarios

**GIVEN** a client IP that has exceeded its per-IP rate limit
**WHEN** `POST /api/inspect` is called with a valid URL
**THEN** the response is `429 Too Many Requests` and no DNS resolution is attempted (rate-limit fires before `validate_target`)

**GIVEN** the enrichment service is unreachable
**WHEN** an inspection completes
**THEN** a `WARN` log line appears with `error` field and message `"enrichment lookup failed"`; the inspection response is returned with `enrichment.ip` present and `enrichment.org` absent

**GIVEN** a successful URL inspection
**WHEN** Prometheus is scraped on `:9090/metrics`
**THEN** `inspect_requests_total{outcome="success"}` has incremented by 1

**GIVEN** a keyboard user navigates to the results page
**WHEN** they press Tab to reach the clear button in `UrlInput`
**THEN** the clear button receives focus and can be activated with Enter/Space

**GIVEN** the results page is rendered with all 9 section cards
**WHEN** the DOM is inspected
**THEN** every `.section-card__header` button has `aria-controls` whose value matches the `id` on the corresponding `.section-card__body` div

**GIVEN** `GET /api/meta` is called
**WHEN** the service has no `[meta]` config section
**THEN** the response is `{"version":"<version>","site_name":"spectra"}` with no `ecosystem` key

**GIVEN** `validate_target` is called with `Url::parse("https://[::1]").unwrap()`
**WHEN** the SSRF check runs
**THEN** it returns `Err(AppError::BlockedTarget(_))` without making a DNS query

**GIVEN** two different client IPs both targeting the same hostname
**WHEN** combined calls exhaust the per-target burst (10 calls)
**THEN** the next call from either IP to that hostname returns `Err(AppError::RateLimited { .. })`

**GIVEN** `downloadJson` is called with `final_url = "not-a-url"`
**WHEN** the export function runs
**THEN** no uncaught exception is thrown and the download proceeds with a fallback filename

**GIVEN** `copyMarkdown` is called on a result with 2 redirect hops and a CORS report
**WHEN** the clipboard content is inspected
**THEN** it contains a `## CORS` section and a `## Redirects` section with 2 entries

---

## Decision Log

- **R3 (shared probe_client) dropped**: `reqwest::Client` sharing requires that `.resolve()` (per-request host binding) and `Policy::custom` (per-request redirect tracking) be set at build time, not per-request. There is no public API to derive a new client from an existing one while overriding these settings. The per-request `ClientBuilder::build()` call is cheap (does not re-initialize TLS; reqwest uses a shared `hyper` connector internally per client). R3 is removed from implementation scope. The decision is documented here so it is not re-raised.

- **`/api/meta` vs expanding `/api/config`**: Chose a dedicated `/api/meta` endpoint to match the pattern used by other services in the suite. The frontend's `fetchMeta()` in `src/lib/api.ts` already targets `/api/meta`.

- **Inline style class naming in R18**: New classes use component-prefixed names (`cache-*`, `cors-*`, `fingerprint-*`, `redirect-*`) to avoid collisions with existing global classes. The `.section-label` class is shared if it already exists.

- **COOP/COEP/CORP explanation text**: Hardcoded strings are specified in R17 rather than wired from `qualityChecks` to avoid the untestable OR clause from the original SDD. The strings are concrete and verifiable.

- **`EnrichmentData::Default` simplification**: Manual `Default` impl (`org: None, ip_type: None, threat: None, role: None`) is identical to what `#[derive(Default)]` produces for `Option<T>` fields. Safe to replace.

- **`body_read_limit_bytes` documented as reserved**: Per completeness finding, documenting an unimplemented config key risks operator confusion. The comment in `spectra.example.toml` explicitly states "has no effect in the current version" to prevent silent misconfiguration.

- **`spectra.toml` config file pattern**: Chose to delete `spectra.toml` from the repo and introduce `spectra.dev.toml` (gitignored) alongside the existing `spectra.example.toml`. Pattern across the suite: `*.example.toml` is the annotated reference (in repo); `*.dev.toml` is the gitignored local dev config; operators create their own production config. Rejected keeping `spectra.toml` with a placeholder because any file in the repo risks accidental deployment without customisation.

---

## Open Decisions

None.

---

## Out of Scope

- Frontend test setup (vitest) — tracked separately
- `network_role` / `role` field wiring — blocked on netray-common ≥ 0.5.5
- Global rate-limit bucket — architectural decision, requires separate SDD
- `ready_handler` dependency probing — requires design for what to probe
- `SectionCard` component extraction from `App.tsx` — refactor, not a correctness fix
- `security_headers_layer` per-request rebuild — needs netray-common API audit; moved fully to out of scope (was incorrectly listed as both R2 and out of scope in original SDD)
- Shared `reqwest::Client` for probe requests — per-request construction is correct; see Decision Log
