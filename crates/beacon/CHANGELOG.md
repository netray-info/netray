# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1] - 2026-05-01

### Changed
- Bump @netray-info/common-frontend to 0.5.2
- Bump netray-common to 0.8.1

## [0.3.0] - 2026-04-25

### Changed (BREAKING)

- `GET /api/meta` response shape now matches the suite-wide `EcosystemMeta`
  contract from `netray-common`. The legacy top-level `service` key is
  renamed to `site_name`. The `ecosystem` object is always present (sibling
  URLs default to empty strings when unset) and three new top-level fields
  are added: `features`, `limits`, `rate_limit`. Any client that read
  `body.service` must now read `body.site_name`.
- `[backends]` configuration flattened. `BEACON_BACKENDS__IP__URL` is
  removed; use `BEACON_BACKENDS__IP_URL` (single underscore between
  segments). `BEACON_BACKENDS__IP__TIMEOUT_MS` is replaced by a shared
  `BEACON_BACKENDS__TIMEOUT_MS`. The TOML form is `[backends] ip_url = "…"`,
  `timeout_ms = 5000`.

### Refactored

- `frontend/src/App.tsx` decomposed from 1008 lines to 293; new components
  in `frontend/src/components/` (`DomainInput`, `CategoryCard`,
  `ResultsGrid`, `SummaryCard`, `GradeDisplay`, plus helpers).
- `frontend/src/styles/global.css` now imports the four shared stylesheets
  from `@netray-info/common-frontend` (theme, reset, layout, components).
  Per-tool `--pass`/`--warn`/`--fail`/`--skip` redefinitions removed.

### Added

- Integration test at `tests/enrichment_integration.rs` (wiremock-based)
  asserting that setting `BEACON_BACKENDS__IP_URL` causes the enrichment
  client to issue an outbound HTTP request.

## [0.2.0] - 2026-04-23

### Added

- Production-readiness review covering SDD tracks A-I: trusted-proxy
  client-IP extraction, SSE abort-on-disconnect, concurrency semaphore
  with `TOO_MANY_INSPECTIONS`, SSRF guard on BIMI logo fetches, MTA-STS
  body/MX caps, redacted domain in error logs, per-task panic counters,
  DnsLookup trait for test injection, tagged `Phase0Result`/`Phase1Result`,
  `Grade::Skipped`, error banner + aria-live in frontend (9b2dbef)
- Surface full check detail in expanded card body (a46fe8d)
- Cross-tool header link for DKIM and DANE cards (2129ddd)
- DNSSEC cross-reference link to tls.netray.info (158714f)

### Changed

- Project-review remediations: swap jsdom for happy-dom with
  vitest.workspace.ts and test-setup.ts, extract `lib/history.ts`,
  canonical `[telemetry]` block in `beacon.toml.example`, move
  cargo-machete from clippy gate to audit job (03a4417)
- Unify domain + DKIM selectors into single query input (300e03b)
- Inline cross-tool header links and per-card explainers (d3a1d24)
- Apply cargo fmt (78c6af0)
- Update .npmrc for GitHub Packages auth (7f171e8)

### Fixed

- mx: downgrade single_mx to Info when multiple IPs resolve (f8a4159)
- dnssec: query DNSKEY instead of RRSIG to detect signed zones (04eecc5)
- cross-validation: relax spf_mx_coverage verdict (f6bf8d7)
- dnsbl: classify response codes and use a separate resolver (0a50734)
- frontend: bump @netray-info/common-frontend to ^0.5.0 (1072cda)
- frontend: regenerate lockfile after package.json bump (6e0fcd0)

## [0.1.0] - 2026-04-11

Initial release of beacon, the email security inspector for netray.info.

### Added

- Email security posture analysis with 12 check categories: MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, DNSBL, and cross-validation
- Aggregate A-F grading from category verdicts
- SSE streaming results API with duration tracking
- IP enrichment via netray-common EnrichmentClient
- SolidJS frontend with shimmer skeleton loading, help modal, keyboard navigation
- Overview card with badge pill counts above section list
- Human-readable sub-check labels with explain toggle
- Duration display, share button, copy as Markdown/JSON
- Categories grouped into semantic sections
- Notice banner when domain does not receive email
- OpenAPI 3.1 docs via utoipa + Scalar UI at `/docs`
- Meta endpoint with ecosystem info
- Prometheus metrics on admin port
- TOML config with env var overrides (`BEACON_` prefix)
- GitHub Actions CI/CD and deploy workflows
- DNS provider filtering for IPv4-only; support for explicit IPs
- Tracing correlation for on_response log with request span
- Frontend CSS variable usage for shimmer animation
- SuiteNav placement and skip-link accessibility
- Renamed project from mail-inspector to beacon

[0.2.0]: https://github.com/netray-info/beacon/releases/tag/v0.2.0
[0.1.0]: https://github.com/netray-info/beacon/releases/tag/v0.1.0
