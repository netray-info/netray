# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[0.1.0]: https://github.com/netray-info/beacon/releases/tag/v0.1.0
