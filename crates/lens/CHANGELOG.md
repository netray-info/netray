# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
