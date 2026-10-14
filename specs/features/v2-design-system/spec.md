# Spec: V2 design system (Phase 4a)

Status: Draft
Created: 2026-10-14

## Goal

A library crate `netray-web` at `crates/web` generates `tokens.css` and a Rust `tokens` module from `crates/web/tokens.json`, proves every status colour's contrast, holds one askama macro and one stylesheet per R44 component, the W5b logo, the favicon, the badge mark and the tagline partial, and runs a gate test that fails on a colour, radius, font, font size or line width outside the tokens, on a second implementation of a component, and on an inline script; `just adlc-verify` runs it all.

## Non-goals

- No route, no page, no change to the `netray` binary or to any wire (V4.4–V4.13, Phase 4).
- No islands bundle, no esbuild, no vitest (§8.4 comes with the pages).
- No rewrite of `specs/rules/frontend-rules.md` (§8.6, Phase 4).
- The Solid frontends and `site/` stay outside the gate until Phase 4 retires them.
- The P03 name grep of §8.7 (templates, API serializers, site output) comes with the pages.

## Context and constraints

- SDD `specs/sdd/v2.md`: S38 (scope), §8.1 (layout), §8.2 (tokens, contrast), §8.3 (components), §8.5 (gate), §8.7 (identity), V4.1–V4.3 and V4.14 (`specs/sdd/v2.md:1083-1096`).
- Token source today: `specs/sdd/v2-tokens.json`; `tests/repo/test_sdd_in_repo.sh` checks it there. Every status colour already meets the §8.2 limits against `bg` and `plate` in both themes (computed 2026-10-14 with the WCAG formula).
- Workspace: every `crates/*` is a member (`Cargo.toml:2`); `tests/repo/test_cargo_workspace.sh` C1 wants every crate but common, model and engine to depend on `netray-common` by path; the SDD lists `netray-common` (middleware, rate limiter) as what `web` will use.
- askama 0.16.1 is in the local cargo cache; it is not yet a workspace dependency.
- Design reference: the PRD's design canvas (private), boards "D3 · W5 Scale, developed" (W5b = "Marker through"), "Wordmark component" (variant `s2`), "D3 · Tokens", the D3 result, inspector and mobile boards for component markup.
- Gate: `just adlc-verify` runs `cargo test --workspace --exclude netray-ip`, so a test in `netray-web` runs without justfile changes.

## Requirements

1. `crates/web` is the package `netray-web`, edition 2024, a library with no route, depending on `netray-common` by path and on askama 0.16 as a workspace dependency; the `netray` binary does not depend on it.
2. `crates/web/tokens.json` is the token source, moved byte for byte from `specs/sdd/v2-tokens.json`; §8.1 of the SDD, `CLAUDE.md` and `tests/repo/test_sdd_in_repo.sh` name the new path, and the old file is gone.
3. `build.rs` writes `tokens.css`: light values on `:root`, dark values under `@media (prefers-color-scheme: dark)` and under `[data-theme="dark"]`, the phone values (`font.size.h1Phone`, `gradePhone`, `captionPhone`, `spectrum.plateHeightPhone`, `traceRowPhone`) under a `max-width` query at `layout.phone`; each custom property is named after its JSON path without the theme segment (`--status-warn-text`, `--line-marker`, `--color-bg`). The crate exposes it as `netray_web::tokens::CSS`.
4. `build.rs` writes a Rust `tokens` module with one `&str` constant per token value and theme, for the OG and badge renderers.
5. A test computes the WCAG contrast of every status colour against `color.<theme>.bg` and `color.<theme>.plate` in both themes: `mark` and `hair` at least 3:1, `text` at least 4.5:1; it fails naming the colour, the ground and the ratio.
6. A gate test scans every file under `crates/web/templates`, `crates/web/static` and `crates/web/islands` and fails when (1) a colour literal (hex, `rgb()`/`rgba()`, `hsl()`/`hsla()`, a CSS named colour), or a `border-radius`, `font-family`, `font-size`, `stroke-width` or border width that is not a `var(--…)`, appears outside the token source; (2) a component's class name is defined in a stylesheet other than its own, or appears in a template other than its own macro file; (3) a template holds an inline `<script>` other than `<script type="speculationrules">`. Each rule has a planted-violation fixture the test must refuse.
7. Each R44 component (`grade`, `run_line`, `progress_ray`, `spectrum`, `trace`, `check_row`, `evidence_block`, `diff_block`, `grouping_switch`, `query_input`) exists once as a macro in `templates/components/<name>.html` and once as `static/<name>.css`, typed against its own view struct in `netray-web`; a test renders each from a fixed view and asserts its markup; `static/base.css` holds the marker rule (R43).
8. `static/logo.svg` is W5b: "net" regular and "ray" bold as SVG `<text>` in the `font.sans` stack, a tick scale under each, the marker line through the scale axis between them; `static/favicon.svg` and `static/badge-mark.svg` are the marker crossing the axis on a tile with at most 2 px radius. All three are monochrome: one ink colour, no status colour, no gradient.
9. `templates/partials/tagline.html` holds the one tagline, "Every check of a domain, in one spectrum."; no other template or stylesheet contains it.

## Phase 1 — Tokens

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5

### Test Scenarios

- GIVEN `crates/web/tokens.json` WHEN the crate builds THEN `tokens::CSS` holds `--color-bg: #f7f8fa` on `:root`, `--color-bg: #07080a` under both dark selectors, and `--font-size-h1: 26px` inside the phone query.
- GIVEN a token named `status.light.warn.text` WHEN the CSS is generated THEN its property is `--status-warn-text`.
- GIVEN the real tokens WHEN the contrast test runs THEN it passes; GIVEN a status `text` colour of `#9aa0a6` on the light `bg` THEN it fails naming that colour.
- GIVEN the move WHEN `tests/repo/test_sdd_in_repo.sh` runs THEN it passes with `crates/web/tokens.json` and fails when that file is missing.

## Phase 2 — The gate

**Depends on:** Phase 1
**Requirements:** 6

### Test Scenarios

- GIVEN the crate's real templates and stylesheets WHEN the gate runs THEN it passes.
- GIVEN a planted stylesheet with `color: #fff` THEN the gate fails on rule 1; GIVEN `border-radius: var(--radius-max)` THEN it passes.
- GIVEN a planted stylesheet that defines `.check-row` outside `check_row.css` THEN the gate fails on rule 2.
- GIVEN a planted template with `<script>alert(1)</script>` THEN the gate fails on rule 3; GIVEN `<script type="speculationrules">` THEN it passes.

## Phase 3 — Components

**Depends on:** Phase 2
**Requirements:** 7

### Test Scenarios

- GIVEN a fixed view per component WHEN its macro renders THEN the markup carries the component's root class and the view's values.
- GIVEN the ten components WHEN the gate runs THEN it passes.

## Phase 4 — Identity

**Depends on:** Phase 2
**Requirements:** 8, 9

### Test Scenarios

- GIVEN the three SVGs WHEN a test reads them THEN each uses one fill/stroke colour, no status colour from the tokens, no gradient, and no radius over 2 px.
- GIVEN the tagline WHEN a test searches templates and stylesheets THEN it appears only in `templates/partials/tagline.html`.

## Decision log

- One feature with four phases over four features (operator, 2026-10-14).
- A library without a route over a preview route (operator, 2026-10-14).
- Component markup, logo and marks from the design canvas (operator, 2026-10-14).
- Logo text as SVG `<text>` in the system stack over outlined paths: R41 loads no web font, and no font file exists to outline from (operator, 2026-10-14).
- Favicon and badge tile at most 2 px radius, over the canvas's 7/4 px or no tile: the SDD (R43, V4.14) wins (operator, 2026-10-14).
- Component macros typed against view structs in `netray-web` over `netray-model` types: 4a stays independent of history and API (operator, 2026-10-14).
- Tagline "Every check of a domain, in one spectrum." (operator, 2026-10-14).
- `netray-web` depends on `netray-common` now, over adding it to the C1 exception list: the SDD has it use common's middleware and rate limiter, and the workspace rule stays unchanged (implementer).

## Open decisions

None.

## Out of scope

- Pages, routes, islands, the rewrite of `frontend-rules.md`: SDD Phase 4 (V4.4–V4.13).
