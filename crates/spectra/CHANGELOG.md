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
