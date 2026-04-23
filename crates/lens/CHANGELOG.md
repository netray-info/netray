# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Email security section powered by the beacon backend (`LENS_BACKENDS__EMAIL__URL`)
- Four scored buckets: `email_authentication` (always), `email_infrastructure`, `email_transport`, `email_brand_policy` (N/A when no MX records)
- `SectionStatus::NotApplicable` in the scoring engine; `OverallScore.not_applicable` map in summary event
- `dkim_selectors` query parameter on `GET /api/check/{domain}` and POST body field for custom DKIM selector testing
- `email` SSE event with bucket verdicts, N/A flags, beacon grade, and detail URL
- `summary.not_applicable` field (always serialized; non-empty when a section is N/A)
- `SectionError::NotApplicable` variant for beacon-Skipped sentinel
- `EmailSection` frontend component with four bucket rows and N/A state rendering
- N/A footnote in Summary when `not_applicable` is non-empty

### Changed
- Scoring weights: TLS 35% (was 40%), DNS 20% (was 30%), Email 15% (new), HTTP 20%, IP 10%
- DNS section no longer scores SPF, DMARC, MTA-STS, TLS-RPT, BIMI, MX — transferred to email backend
- DNS `hard_fail` cleared (SPF/DMARC hard-fails removed; email hard-fails deferred)
- DNS headline now shows DNSSEC, CAA, NS, CNAME-apex (was SPF, DMARC, DNSSEC, MTA-STS)
- Shared SSE collector extracted to `src/backends/sse.rs`; DNS backend uses it directly

## [0.4.0] - 2026-04-11

### Added
- Add [ecosystem] config for public-facing cross-link URLs (0d450d7)

### Fixed
- Resolve clippy collapsible-if warnings (88c9122)
- Include http_url in startup log (a34e880)

### Changed
- Bump netray-common to 0.7.0 (37a43d1)
- Migrate to per-backend config sub-tables and shared ecosystem (35fe9d2)
- Apply cargo fmt formatting (1d02fac)

## [0.3.0] - 2026-04-10

### Added
- Style example domain buttons and expand-all toggle (f372c90)
- Move validation chips into summary card, fix chip styling (8915b65)
- Tie section dot colors to grade, soften overall label (21a62bc)
- Section dots as tool links, duration labeled in top section (3ec25c2)
- Add share button, modal-close style, source-prefixed server rows, remove HTTP meta bar (6c03bc2)
- Restructure summary card, restyle chips, move actions (832f506)
- HTTP card improvements -- server info, explanations, card order (90680e5)
- Add HTTP section via spectra backend (91ffb73)
- Add MCP skills and update API docs for LLM usage (65b4e3a)
- Add sync mode, OpenAPI docs, hard_fail_reason, rate_limit meta (7de204a)
- Add About section, j/k nav, update tagline and examples (d5e7b74)

### Fixed
- Vertically center grade letter against info block height (1b11f35)
- Fix three summary card alignment issues (ee053d7)
- Thread server_network_type through HTTP backend; fix summary layout (6be0b33)
- Hide section headline when only one check to avoid duplication (0e1816f)
- Set font-weight: 400 on .ext-link to match summary IP link (3c77bdd)
- Unify section and summary external link styling under .ext-link (fa5a323)
- Remove flex from summary-top so actions right-align correctly (7ef8f00)
- Always render summary-actions div; restore section-card link accent color (554afc9)
- Reliably right-align copy MD | JSON via space-between layout (67138cf)
- Match input row proportions to spectra (b9fe6c6)
- Mute section card links to match summary IP link; revert dot-links; rename duration label (680ab8c)
- Push copy MD | JSON actions to far right via flex: 1 on dots (427354b)
- Align dots+actions with score row; color-code duration (beafebf)

### Changed
- Bump netray-common to 0.6.0 (5d79e68)
- Replace hardcoded scoring sections with dynamic HashMap-based engine (68b684e)
- Assign unique dev ports: backend 8085, metrics 9095, vite 5178 (8fe6cf8)

## [0.2.4] - 2026-04-09

### Fixed
- Enrich footer with tech stack description (8bdc46c)
- Full-row check tinting and SuiteNav compaction (deca9aa)
- Align verdict chip border-radius to pill shape (15081d1)
- Remove NODE_AUTH_TOKEN from .npmrc, use global auth (80caf8c)
- Swap header-actions order to match frontend-rules (06f6501)

### Changed
- Remove SuiteNav compaction override, now in common-frontend (b5163ba)
- Bump common-frontend to ^0.5.0 (999eda9)
- Bump common-frontend to ^0.4.0 (a742290)

## [0.2.3] - 2026-04-09

### Changed
- Move health and ready probes to root-level paths (31c51a5)
- Condense CLAUDE.md rules and principles to avoid global duplication (06e6a6f)
- Deduplicate frontend-rules and update spec paths in CLAUDE.md (514e7bf)

## [0.2.2] - 2026-04-09

### Fixed
- Use build_error_response() to eliminate double-logging (6334051)
- Add specific rejection logging per error variant (f5f3a37)
- Log HTTP request completion at INFO level, not DEBUG (59c08f2)

## [0.2.1] - 2026-04-08

### Fixed
- Add span enrichment, backend instrumentation, and startup inventory per logging-rules spec (6420e46)

## [0.2.0] - 2026-04-08

### Added
- Diagnostic messages on check results with guide URL links (f94f576)
- Expand guide URLs to cover all checks and add per-section grades (a5a019b)
- Frontend: primary button, example chips, GitHub link (99d6014)

### Fixed
- Frontend: show error headline instead of check summary on section errors (ba7b10b)
- Frontend: replace explain mode with inline messages and guide links (d3cdc5d)
- Frontend: ErrorBoundary, URL restore timing, history keyboard nav (329a275)
- Missing telemetry field in test Config constructors (9ea615c)
- Cargo fmt violations (373e614)
- CI: add jsdom devDep, fix clippy collapsible_if warnings (95ceb37)

### Changed
- CI: add frontend lint script with tsc --noEmit (414c697)
- CI: align workflows with netray.info workflow-rules spec (f9d0ae5)
- Frontend: bump common-frontend to ^0.3.2 (f41472f, c7e860d)
- Frontend: align tsconfig to canonical spec (a290ccc)

## [0.1.1] - 2026-04-08

### Changed
- Bump typescript 5→6, vite 7→8; fix TypeScript 6 tsconfig compatibility (525b57e)

## [0.1.0] - 2026-04-07

### Added
- Initial release: unified domain health check service (DNS + TLS + IP enrichment)
- SSE streaming with letter-graded health score driven by TOML scoring profiles
- Frontend: SolidJS SPA with expand/collapse, export, cross-tool deep links
