# Plan: v2-design-system

## Phase 1 — Tokens

### Groups

- G1: C2, C10 — the token file moves; SDD and CLAUDE.md name the new path; §3.1 gains `common`.
- G2: C1, C3, C4, C5, C6, C7, C8, C9 — `build.rs`, `src/tokens.rs`, `src/contrast.rs`, `src/lib.rs`, `serde_json` to `[dependencies]`. Shares `lib.rs`, so one group; needs G1's file.

### Plan

- G1: `git mv specs/sdd/v2-tokens.json crates/web/tokens.json` (bytes unchanged, lines 96 and 163 still the plate hairs); `specs/sdd/v2.md` lines 83 (name and Depends-on `common`), 687, 862 (§8.1 row, no old name left), 884 (`crates/web/tokens.json:96`, `:163`); `CLAUDE.md:24` names `crates/web/tokens.json`.
- G2: `build.rs` walks `tokens.json` (skip `$` keys and nodes without `$value`), writes `$OUT_DIR/tokens.css` (`:root` light and theme-less plus glow `none`; dark under `@media (prefers-color-scheme: dark) { :root {…} }` and `[data-theme="dark"]` with the glow shadows; `@media (max-width: <layout.phone>) { :root {…} }` with each `…Phone` overriding its base) and `$OUT_DIR/tokens_values.rs` (UPPER_SNAKE path constants, theme kept, `{:?}`); `src/tokens.rs` includes both; `src/contrast.rs` `ratio` (WCAG 2) and `violations` (mark/hair ≥ 3, text ≥ 4.5 vs bg and plate, both themes, message with path, hex, ground, ratio `{:.2}`); `lib.rs` declares both modules, no `Router` word.

## Phase 2 — The gate

### Groups

- G1: C1, C3, C4, C5 — `crates/web/src/gate.rs`, `crates/web/src/lib.rs`
- G2: C2 (after G1) — `crates/web/static/base.css`

### Plan

**G1.** `crates/web/src/gate.rs`, std only: `Violation { rule, path, line, message }`, `Report { scanned, violations }`, `check(root)`.
- Walk `templates/**/*.html`, `static/**/*.css`, `islands/**/*.ts` with recursive `read_dir`; a missing directory is empty; `scanned` counts matched files; SVGs are never scanned.
- Token set: every `--name:` declared in `tokens::CSS`.
- Rule 1: hex, `rgb[a](`, `hsl[a](`, `oklch(`, `color(` and CSS named colours in declaration values (stylesheets, and in templates only inside `style="…"` and `<style>`, so a fragment id like `href="#add"` is no colour; islands everywhere). Custom-property declarations (`--x: …`) are not themselves flagged. For `border-radius`, `font-family`, `font-size`, `font`, `stroke-width`, `border[-side][-width]`: every `var(--x)` must name a token, and nothing but border-style keywords may remain beside the `var()`s.
- Rule 2: the ten R44 names, class = name with `_` → `-`; a `.<class>` selector outside `static/<name>.css`, or the class in a `class="…"` outside `templates/components/<name>.html`.
- Rule 3: in templates, a `<script` without `src=` and not `type="speculationrules"`.
- `pub mod gate;` in `crates/web/src/lib.rs`.

**G2.** `crates/web/static/base.css`: a `body` rule with tokens only (`--color-bg`, `--color-ink`, `--font-sans`, `--font-size-body`); the R43 marker rule comes in Phase 3.

## Phase 3 — Components

### Groups

- G1: C1, C2, C3 — the ten components share `components.rs`, `lib.rs` and the macro files every test reads; the marker rule in `base.css` is part of C1.

### Plan

**G1.** The API (names, fields, markup contract) as fixed by `crates/web/tests/components.rs`.
- `crates/web/src/lib.rs`: `pub mod components;`.
- `crates/web/src/components.rs`: `Tone`, `SegmentState`, `EvidenceShape` with `as_str()` (no `*Status`/`*Verdict` enum, P03); `RecordRow`, `Field`, `TreeItem`; the ten `*View` structs and their row types (`RaySegment`, `SpectrumLine`, `SpectrumRegion`, `TraceRow`, `DiffEntry`, `GroupingOption`), fields `pub`, `Debug, Clone`; per component a private askama template struct whose inline source imports `components/<name>.html` and calls the macro, and `pub fn <name>(view) -> askama::Result<String>`.
- `crates/web/templates/components/<name>.html` ×10: `{% macro <name>(view) %}` once; root class `<root>`, inner `<root>__<part>`, tone as `data-tone`; no colour, width or font attribute in the spectrum SVG (CSS on `data-tone` draws them); no other component's class; trace rows and grouping-switch options carry `marker`; no script, no `on*`.
- `crates/web/static/<name>.css` ×10: only the component's own selectors; colours `var(--color-*)`/`var(--status-*)`; radius, font size and family, stroke and border widths through their token family; border shorthands state a width.
- `crates/web/static/base.css`: the R43 marker rule on `.marker` for `aria-pressed="true"`, `aria-current` (not `"false"`) and `aria-selected="true"`: a `var(--line-marker)` line in `var(--color-ink)` at the inline start, no component class.
