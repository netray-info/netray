# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.5.2] - 2026-03-15

### Changed

- Live OCSP revocation check wired into the inspection pipeline; `tls_params.ocsp_live` is now populated for certificates with an AIA OCSP URL and an issuer in the chain (previously the implementation existed but was never called)
- UI: added favicon with lock motif

## [0.5.1] - 2026-03-14

### Changed

- CAA issuer matching replaced with a compile-time lookup table generated from SSLMate + CCADB sources (155 entries); eliminates the heuristic and makes unknown CAA domains an explicit Fail
- CI: fixed `cargo-cyclonedx` flag, `deny.toml` deprecated keys, RUSTSEC advisory allowlist, `.dockerignore`
- Added deploy task for release pipeline

## [0.5.0] - 2026-03-14

### Changed

- Bumped version following Phase 2 feature work; removed `prod.toml` from repository (config injected at deploy time)
- Live OCSP check infrastructure added (`OcspRevocationResult` type, `check_live_ocsp` function); returns `"unknown"` until wired in 0.5.2

## [0.4.0] - 2026-03-14

### Added

- **STARTTLS/SMTP** — auto-negotiated on ports 25 and 587 before the TLS handshake; port-based detection, no extra config required
- **AIA URL extraction** — OCSP responder URL and CA Issuers URL extracted from each certificate's Authority Information Access extension and exposed in the API response
- **Live OCSP check stub** — `OcspRevocationResult` type and `check_live_ocsp` function wired into the data model; returns `"unknown"` pending full OCSP request builder implementation
- **ECH detection** — HTTPS DNS record (type 65) queried per hostname; `ech_advertised` health check (Warn if absent on port 443) added to the Configuration category
- **Certificate policy classification** — EV / OV / DV detection via Certificate Policies extension OIDs and subject O field; `cert_policy` field on every `CertInfo`
- **Certificate lifetime health check** — Warn when leaf validity exceeds 398 days; Fail when it exceeds 825 days (CA/B Forum limits)
- **ALPN consistency health check** — detects ALPN protocol mismatches across IPs; Warn on divergence
- **Key exchange group** — negotiated named curve (X25519, P-256, P-384, P-521, X448) captured during handshake and exposed in TLS parameters
- **TLS 1.2 health check verdict** — `tls_version` check now emits Warn (previously Pass) for TLS 1.2
- **`custom_ca_count` in `/api/meta`** — number of custom CA certificates currently loaded, updated on hot-reload
- **Custom CA hot-reload** — `custom_ca_dir` is re-scanned on `SIGHUP`; no restart needed
- **SAN DNS deep-links** — SAN DNS entries in the certificate detail link to the DNS inspector
- **`/api/ready` readiness probe** — dedicated readiness endpoint (separate from `/api/health` liveness)
- **Certificate validity timeline bar** (frontend) — color-coded progress bar per certificate: green > 30 days remaining, amber 1–30 days, red expired
- **OCSP staple freshness badge** (frontend) — shows staple age and validity window alongside the staple status badge
- **Copy-to-clipboard buttons** (frontend) — individual certificate field values (subject, serial, fingerprints, AIA URLs, …) have copy buttons
- **Cert change detection banner** (frontend) — banner shown when the leaf certificate fingerprint differs from the last visit; diff stored in `localStorage`
- **Port tab verdict badges** (frontend) — colored dot per port tab derived from the port's health check roll-up (pass / warn / fail)

### Changed

- Bumped `netray-common` to v0.4.1
- Migrated to `netray-common-frontend` v0.2.0 (published to GitHub Packages)
- Per-port IP inspection parallelized with `JoinSet`; startup warning emitted when `allow_blocked_targets = true`
- `enrichment.rs` migrated to `netray_common::enrichment`; server, telemetry, CORS, middleware, and `ip_filter` modules migrated to `netray-common` equivalents
- DANE validation wired; A/AAAA lookups use the `mhost` resolver consistently
- `netray-common` v0.3.0 published to crates.io; `[patch.crates-io]` override removed

## [0.3.2] - 2026-02-xx

### Changed

- Bumped `quinn-proto` to 0.11.14 (RUSTSEC-2026-0037)
- Frontend: switched vitest environment from jsdom to node
- CI: added `NODE_AUTH_TOKEN` for `npm ci` in frontend job
- Docker: added `workflow_dispatch` trigger to release workflow

## [0.3.1] - 2026-02-xx

### Changed

- Fixed `cargo fmt` formatting in `rate_limit.rs`
- Migrated frontend dependency from sibling-repo relative path to `@netray-info/common-frontend` on GitHub Packages

## [0.3.0] - 2026-01-xx

### Added

- Optional OpenTelemetry OTLP tracing
- Playwright E2E test suite
- Configurable site name and footer text
- IP enrichment via `ifconfig-rs` and custom CA loading
- `allow_blocked_targets` config option for internal testing
- Hot reload via ArcSwap + SIGHUP for configuration
- OpenAPI spec via `utoipa` + Scalar UI at `/docs`

### Changed

- Bumped `netray-common` to v0.2.1 / v0.2.0 (CIDR trusted-proxy support)
- Unified IP view; collapsible sections; keyboard navigation
- Adopted `netray-common` for security headers, error infrastructure, and rate limiting
