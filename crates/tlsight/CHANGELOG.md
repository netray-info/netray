# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.12.1] - 2026-05-01

### Security
- Bump frontend lockfile for postcss XSS (GHSA-qx2v-qp2m-jg93)

### Changed
- Bump @netray-info/common-frontend to 0.5.2
- Bump netray-common to 0.8.1
- Add CONTRIBUTING.md and DCO sign-off CI
- Drop monitoring-service example from licensing section

## [0.11.1] - 2026-04-11

### Fixed
- Resolve clippy collapsible-if warning in readiness check (7b79f72)

### Changed
- Migrate to standardized ecosystem/backend config (6857b78)

## [0.11.0] - 2026-04-10

### Added
- Add guide links in explain mode (fd43e0c)
- Add About section to help modal, convert to shortcuts-table (abf4331)

### Changed
- Bump netray-common to 0.6.0 (76ea295)
- Assign unique dev ports: backend 8082, metrics 9092, vite 5175 (e37ed0d)

## [0.10.4] - 2026-04-09

### Added
- Check row tinting and deep links in section headers (aaddb56)

### Fixed
- Remove NODE_AUTH_TOKEN from .npmrc, use global auth (56b7230)
- Add SuiteNav compaction override (c663c18)

### Changed
- Remove SuiteNav compaction override, now in common-frontend (5db70bd)
- Move SuiteNav inside .app (51a1edf)
- Bump common-frontend to ^0.5.0 (66484f3)
- Bump common-frontend to ^0.4.0 (de02257)

## [0.10.3] - 2026-04-09

### Changed
- Move health and ready probes to root-level paths (9688854)
- Condense CLAUDE.md rules and principles to avoid global duplication (2560065)
- Deduplicate frontend-rules and update spec paths in CLAUDE.md (3944822)

## [0.10.2] - 2026-04-09

### Fixed
- Use build_error_response() to eliminate double-logging (4554947)
- Add max_concurrent to startup inventory log (9252fb6)
- Log HTTP request completion at INFO level, read request_id from header (76eb907)

## [0.10.1] - 2026-04-08

### Added
- Standardize default log filter (7a38025)

### Fixed
- Add span enrichment, rejection logging, and startup inventory per logging-rules spec (121b909)

## [0.10.0] - 2026-04-08

### Fixed
- Frontend: SolidJS pattern violations and CSS correctness (dee5f38)

### Changed
- CI: add frontend lint script with tsc --noEmit (de56602)
- CI: align workflows with netray.info workflow-rules spec (4fbe08c)

## [0.9.0] - 2026-04-08

### Added
- Primary button uses shared `.btn-primary`; replace CrossLinks with shared CrossLink component; fix hardcoded DNS URL to use meta ecosystem URL (ed34020)
- Landing mode cards adopt shared `.mode-card` classes from common-frontend (b24be1b)
- ValidationSummary chips derive base shape from shared `.filter-toggle` (87df3cb)

### Fixed
- Pass `label` prop to CrossLink instead of children (cb28ea8)

## [0.8.1] - 2026-04-08

### Changed
- Bump webpki-roots 0.26→1, x509-parser 0.17→0.18, sha1/sha2 0.10→0.11, rcgen 0.13→0.14 (4954de0)
- Bump reqwest 0.12→0.13, typescript 5→6, vite 7→8 (bfaf414, 923e246)
- Fix rcgen 0.14 API: rename CertifiedKey.key_pair to signing_key in tests (4954de0)

## [0.8.0] - 2026-04-07

### Added
- SAN quality, AIA reachability checks, deep links, mobile UX, trace ID propagation (2f86129)
- MIT LICENSE file (a69d2b2)

### Changed
- Frontend: use shared SuiteNav, history factory, fetchWithTimeout from common-frontend (de61a7a)
- Frontend: bump @netray-info/common-frontend to ^0.3.0 (e2a7dad)
- Add human-readable docs link to OpenAPI description; CI integration examples in README (4ce2442)

## [0.7.2] - 2026-04-07

### Fixed
- Relax common-frontend version to ^0.2.1 (a614a89)
- Upgrade vite 7.3.1 → 7.3.2 (CVE dev-server vulns) (f3be4e2)

## [0.7.1] - 2026-04-06

### Added
- Default theme to system preference (a62aadb)

### Changed
- Bump @netray-info/common-frontend to 0.2.2 (1de39c7)
- Update Cargo.lock (c315fd5)

## [0.7.0] - 2026-04-06

### Added
- Suite nav, TLS→DNS cross-links, Cache-Control headers, OpenAPI CORS note (4dceaa7)
- Explicit robots.txt route returning text/plain before SPA fallback (4dceaa7)

### Fixed
- Move SuiteNav outside .app to span full viewport width (81dfce3)

### Changed
- Replace ValueProp card with extended subtitle line (a33115c)
- body font-size aligned to 16px across suite (4dceaa7)
- Meta tags: title, OG, Twitter cards added to index.html (4dceaa7)

## [0.6.0] - 2026-03-15

### Fixed

- **CAA false fail**: CAA `issue` values wrapped in literal double quotes (a common DNS editor mistake) now match correctly — surrounding `"` are stripped before the issuer lookup
- **OCSP staple false warn**: `ocsp_stapled` check now returns Skip (not Warn) when the issuing CA's certificate contains no AIA OCSP URL; affects all Let's Encrypt certs from intermediates R10–R14 and E5–E6 which use CRL-only revocation

### Changed

- **Badge row**: `dane_valid`, `ct_logged`, `ocsp_stapled`, and `consistency` badges are now hidden in the compact summary row when their status is Skip — they appear only when they carry actual signal (pass/warn/fail)
- **ALPN consistency detail**: `"consistent: none"` replaced with `"all IPs: no ALPN negotiated"` (or `"all IPs: {proto}"` when a protocol is negotiated)
- **CAA issuer table**: refreshed from SSLMate + CCADB sources
- Dev config: CT checking enabled by default (`check_ct = true`)

### Fixed (UI)

- Added missing expand-mode explanations for `ech_advertised` and `cert_lifetime` checks
- Added badge tooltip for `alpn_consistency`
- Corrected `tls_version` explanation (warns on TLS 1.2 since 0.4.0, does not pass)

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
