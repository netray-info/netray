# Changelog

All notable changes to spectra are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.3.0] - 2026-10-07

### Changed (BREAKING)
- Every config struct rejects unknown keys (`deny_unknown_fields`): a typo or stale key in `spectra.toml` or a `SPECTRA__*` variable now fails the load instead of being ignored (11a8867)

### Fixed
- `/api/meta` serves `email_base_url` from the `[meta]` config instead of an empty string (e511570)
- `detail_url` in inspect results is built from the public `meta.ip_base_url`, not the internal backend URL (925b5f3)

### Changed
- CI: advisory scans (RUSTSEC, npm audit) moved from the PR gate to a daily scheduled `audit.yml`, which may open issues (c5b5ccf, c585b25)
- `justfile` replaces the Makefile (8701f37); CONTRIBUTING.md and DCO sign-off CI added (9ff5c81, 16d615a)

## [0.2.3] - 2026-05-01

Note: v0.2.1 and v0.2.2 were never released; v0.2.2 exists as a dangling
tag on the remote pointing at an abandoned commit chain that was reset.

### Fixed
- Hoist `analyze_reporting` and `format_http_version` above their `#[cfg(test)]` modules; drop unused `mut` and replace `len() >= 1` with `!is_empty()` (newer clippy)

### Changed
- Assign unique dev ports (backend 8083, metrics 9093, vite 5176) to avoid conflicts when running multiple tools in parallel
- Bump @netray-info/common-frontend to 0.5.2
- Bump netray-common to 0.8.1

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
