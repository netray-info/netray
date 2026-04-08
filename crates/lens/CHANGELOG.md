# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.1] - 2026-04-08

### Changed
- Bump typescript 5→6, vite 7→8; fix TypeScript 6 tsconfig compatibility (525b57e)

## [0.1.0] - 2026-04-07

### Added
- Initial release: unified domain health check service (DNS + TLS + IP enrichment)
- SSE streaming with letter-graded health score driven by TOML scoring profiles
- Frontend: SolidJS SPA with expand/collapse, export, cross-tool deep links
