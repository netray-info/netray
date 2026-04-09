# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-04-09

### Added
- Verdict badge CSS components (`.verdict-badge--pass/fail/warn/skip`, filled variants)
- Section card CSS components (`.section-card` with header, status dot, chevron, body)
- Check row CSS components (`.check-row` with fail/warn background tinting)
- `--pass`, `--fail`, `--warn`, `--skip` semantic color tokens in both themes

### Changed
- SuiteNav: compact styling (rounded border, smaller font/padding, 32px min-height on desktop, 44px preserved on mobile)

## [0.3.3] - 2026-04-08

### Changed
- CI: remove npm run lint — script does not exist in package.json (f0c9151)
- CI: align workflows with netray.info workflow-rules spec (502d93b)

## [0.3.1] - 2026-04-08

### Changed
- Bump typescript 5→6, vitest 2→4, vite 7→8 (1600f18, 545a75c, f3eb723)
- Add happy-dom devDependency; switch vitest environment to happy-dom for DOM tests (f3eb723)

## [0.3.0] - 2026-04-07

### Added
- SuiteNav component: meta-driven cross-tool navigation with BEM classes, aria-current, fallback URLs (cc4c955)
- createQueryHistory factory: deduped, capped history with storageGet/storageSet integration (cc4c955)
- fetchWithTimeout: AbortController-based fetch helper (cc4c955)
- Shared CSS: header system, .filter-toggle variants, .skip-link, .sr-only (cc4c955)
- MIT LICENSE file (1bc5459)

## [0.2.2] - 2026-04-06

### Fixed
- Replace neon semantic colors with Tailwind equivalents (070ad27)
  - `--success: #44ff44` → `#22c55e`
  - `--warning: #ffaa00` → `#f59e0b`
  - `--error:   #ff4444` → `#ef4444`

## [0.2.1] - 2026-03-11

### Changed
- Remove dead CrossLinks component (fe4c918)

## [0.2.0] - 2026-03-11

### Added
- TypeScript declaration build step via `tsconfig.build.json` (221e1fb)
- Shared CrossLinks component for cross-tool navigation (d0db114)
- Shared CSS exports: reset, layout, components (8c090a8)
- Shared TypeScript utilities: storage, utils (8c090a8)
- ThemeToggle, Modal, SiteFooter shared SolidJS components (fd65593)
- README documenting exports, install, and usage (10889f7)

### Fixed
- Move CSS files to package root styles/ for direct PostCSS resolution (696f70a)

## [0.1.0] - 2026-02-28

### Added
- Initial release: theme.css, reset.css with CSS custom properties for dark/light mode
