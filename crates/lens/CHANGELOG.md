# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.7.1] - 2026-04-25

### Fixed
- `[site]` config: a partial TOML override (e.g. setting only `og_image`) used to reset every other field to `None`, producing an empty `<title>`, `og:title`, `og:description` and `og:site_name` at the apex. Each field now carries its own `#[serde(default = "...")]` so missing fields inherit `SiteConfig::default()` regardless of whether the `[site]` table is present.

## [0.7.0] - 2026-04-25

### Added
- Apex repositioning: lens is now configurable as a public landing product via a 12-field `[site]` config section (`title`, `description`, `og_image`, `og_site_name`, `brand_name`, `brand_tagline`, `status_pill`, `hero_heading`, `hero_subheading`, `example_domains`, `trust_strip`, `footer_about`, `footer_links`). Override per-field with `LENS_SITE__*` env vars (673b4f6).
- `GET /api/meta` exposes `features.site` so the SPA reads branding from the live config instead of hard-coded strings (673b4f6).
- HTML shell templating: `frontend/index.html` placeholders (`{{site_title}}`, `{{site_description}}`, `{{site_og_image}}`, `{{site_og_site_name}}`) substituted once at startup with HTML-escaped values; `og:image` line stripped when value is absent (5a96913).
- Public `Landing` SolidJS component for the idle apex state: hero heading + subheading + status pill, domain input, example chips, six grade descriptors (A+ through F, exact wording in `gradeLegend.ts`), trust strip, and a "Want to look deeper? → Raw data for every check" callout pointing at `/tools` (32d0fac).
- `CheckItem` SSE/sync wire fields `fix_hint` and `fix_owner` with skip-when-empty serialization; `CheckList` renders a remediation block (hint, "Fix: <owner>", "Learn more →") when populated. Per-check copy ships empty in 0.7.0 — content lands incrementally in later releases (e34e3d0).
- Result-state Summary now shows the grade descriptor inline (e.g. "ok — weaknesses worth fixing") and a count-driven headline ("4 things to fix and 6 warnings worth a look") instead of static status labels (ac2c721).
- Persistent deeper-callout below the section grid in result state, so users with failures always have a visible path to the raw inspectors (ac2c721).
- README "Customizing the apex landing" section documenting the `[site]` surface (3199950).

### Fixed
- Hero now renders above the domain input on the idle landing (eef9da5).

### Changed
- Hero typography polished: heavier weight, tighter tracking, accent-gradient rule under the heading; status pill padding refined (eef9da5).
- Grade legend uses the project-wide `--grade-*` palette so B and C read distinctly (lime / amber); same colors as the verdict dots in result state (eef9da5).
- Default `hero_subheading` now mentions IPs alongside DNS/TLS/HTTP/email (eef9da5).
- Default `example_domains` leads with `netray.info` as a self-demo (9442683).
- Pre-existing clippy lints fixed: useless `format!` in `email.rs`, items-after-test-module in `ip.rs`, dead `not_found` helper in `tests/scoring_regression.rs`. No behavior change (c3913d4).

## [0.5.0] - 2026-04-23

### Added
- Email security section powered by the beacon backend (`LENS_BACKENDS__EMAIL__URL`)
- Four scored buckets: `email_authentication` (always scored), `email_infrastructure`, `email_transport`, `email_brand_policy` (N/A when no MX records)
- `SectionStatus::NotApplicable` in the scoring engine; `OverallScore.not_applicable` map in summary event
- `dkim_selectors` query parameter on `GET /api/check/{domain}` and POST body field for custom DKIM selector testing
- `email` SSE event; `summary.not_applicable` field (always serialized)
- `SectionError::NotApplicable` variant for beacon-Skipped sentinel
- `EmailSection` frontend component using shared `CheckList`/`SectionHeadline` components
- N/A footnote in Summary when `not_applicable` is non-empty

### Fixed
- Email backend: handle beacon SSE streams that omit `event:` type lines (all events arrived with empty type)
- `EmailSection`: replace bespoke bucket rendering with standard `CheckList`/`SectionHeadline` components matching other sections

### Changed
- Scoring weights: TLS 35% (was 40%), DNS 20% (was 30%), Email 15% (new), HTTP 20%, IP 10%
- DNS section no longer scores SPF, DMARC, MTA-STS, TLS-RPT, BIMI, MX — transferred to email backend
- DNS `hard_fail` cleared; DNS headline now shows DNSSEC, CAA, NS, CNAME-apex
- Section card order follows tech-stack layers top-down: Email → HTTP → TLS → DNS → IP
- Summary dots order follows stack bottom-up left-to-right: IP → DNS → TLS → HTTP → Email
- Shared SSE collector extracted to `src/backends/sse.rs`; DNS backend uses it directly
- `lens.example.toml` and `lens.dev.toml` updated to current per-backend sub-table config structure

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
