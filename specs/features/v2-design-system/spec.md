# Spec: V2 design system (Phase 4a)

Status: Done
Created: 2026-10-14
Finished: 2026-10-15

## Goal

A library crate `netray-web` at `crates/web` generates `tokens.css` and a Rust `tokens` module from `crates/web/tokens.json`, proves every status colour's contrast, holds one askama macro and one stylesheet per R44 component, the W5b logo, the favicon, the badge mark and the tagline partial, and runs a gate test that fails on a colour, radius, font, font size or line width outside the tokens, on a second implementation of a component, and on an inline script; `just adlc-verify` runs it all.

## Non-goals

- No route, no page, no change to the `netray` binary or to any wire (V4.4–V4.13, Phase 4).
- No islands bundle, no esbuild, no vitest (§8.4 comes with the pages).
- No rewrite of `specs/rules/frontend-rules.md` (§8.6, Phase 4).
- The Solid frontends and `site/` stay outside the gate: `site/` becomes templates in Phase 4 (V4.8), the Solid apps are deleted in Phase 5 (V5.1).
- The P03 name grep of §8.7 (templates, API serializers, site output) comes with the pages.

## Context and constraints

- SDD `specs/sdd/v2.md`: S38 (scope), §8.1 (layout), §8.2 (tokens, contrast), §8.3 (components), §8.5 (gate), §8.7 (identity), V4.1–V4.3 and V4.14 (`specs/sdd/v2.md:1083-1096`).
- Token source today: `specs/sdd/v2-tokens.json`; `tests/repo/test_sdd_in_repo.sh` checks it there. Every status colour already meets the §8.2 limits against `bg` and `plate` in both themes (computed 2026-10-14 with the WCAG formula).
- Workspace: every `crates/*` is a member (`Cargo.toml:2`); `tests/repo/test_cargo_workspace.sh` C1 (lines 26-30) wants every crate but common, model and engine to depend on `netray-common` by path. SDD §3.1 (`specs/sdd/v2.md:83`) lists `engine`, `store`, `model` and module crates for `netray-web`, not `common`, though `web` holds "rate limits" and `common` holds the rate limiter (`v2.md:84`).
- The SDD names the token file in four places: `v2.md:83`, `:687`, `:862` (§8.1) and `:884`.
- `status.glow.*` carries no theme segment and is "Dark theme only; never in light" (`v2-tokens.json:213`).
- An SVG fetched as its own image (`/favicon.svg`, `v2.md:629`) cannot resolve custom properties from `tokens.css`.
- askama 0.16.1 is in the local cargo cache; it is not yet a workspace dependency.
- Design reference: the PRD's design canvas (private), boards "D3 · W5 Scale, developed" (W5b = "Marker through"), "Wordmark component" (variant `s2`), "D3 · Tokens", the D3 result, inspector and mobile boards for component markup.
- Gate: `just adlc-verify` runs `cargo test --workspace --exclude netray-ip`, so a test in `netray-web` runs without justfile changes.

## Requirements

1. `crates/web` is the package `netray-web`, edition 2024, a library with no route, depending on `netray-common` by path and on askama 0.16 as a workspace dependency; the `netray` binary does not depend on it.
2. `crates/web/tokens.json` is the token source, moved byte for byte from `specs/sdd/v2-tokens.json`; the SDD's four mentions (`v2.md:83`, `:687`, `:862`, `:884`), `CLAUDE.md` and `tests/repo/test_sdd_in_repo.sh` name the new path, and the old file is gone. The SDD's §3.1 row for `netray-web` gains `common` in its Depends-on column.
3. `build.rs` writes `tokens.css`: light values on `:root`, dark values under `@media (prefers-color-scheme: dark)` and under `[data-theme="dark"]`, the phone values (`font.size.h1Phone`, `gradePhone`, `captionPhone`, `spectrum.plateHeightPhone`, `traceRowPhone`) under a `max-width` query at `layout.phoneMax` (480 px), where each `…Phone` value overrides its base property (`font.size.h1Phone` sets `--font-size-h1`); `status.glow.*` is set only in the two dark blocks, and `:root` sets each glow property to `none`. `:root` declares `color-scheme: light dark` and `[data-theme="dark"]` declares `color-scheme: dark`, so an embedded SVG's `prefers-color-scheme` follows a forced dark theme. Each custom property is named after its JSON path without the theme segment (`--status-warn-text`, `--line-marker`, `--color-bg`). The crate exposes it as `netray_web::tokens::CSS`.
4. `build.rs` writes a Rust `tokens` module with one `&str` constant per token value and theme, for the OG and badge renderers.
5. A test computes the WCAG contrast of every status colour against `color.<theme>.bg` and `color.<theme>.plate` in both themes: `mark` and `hair` at least 3:1, `text` at least 4.5:1; it fails naming the colour, the ground and the ratio.
6. A gate test scans every template (`crates/web/templates/**/*.html`), stylesheet (`crates/web/static/**/*.css`) and island (`crates/web/islands/**/*.ts`) and fails when (1) a colour literal (hex, `rgb()`/`rgba()`, `hsl()`/`hsla()`, `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, `color()`, `color-mix()`, a CSS named or system colour; `transparent` and `currentcolor` are no literal) appears in a stylesheet, in a template's `style` attribute, `<style>` element or SVG presentation attribute, in a custom property's value, or in a string literal of an island (comments never count); or a radius (`border-radius` and its longhands, SVG `rx`/`ry`), `font-family`, `font-size`, `font` shorthand, `stroke-width`, border width (physical and logical sides), `outline` width or `text-decoration-thickness` — as a property, vendor-prefixed or not, or as the SVG presentation attribute of the same name — is anything but a `var(--…)` of its own token family (radius `--radius-*`, font size `--font-size-*`, font family `--font-sans`/`--font-mono`, widths `--line-*`), a CSS-wide keyword (`inherit`, `initial`, `unset`, `revert`), or `0`/`none` for a width or radius; a stylesheet or template never redefines a token's property; (2) a component's class (its root class, or a class starting with `<root>-` or `<root>__`) is defined in a stylesheet other than its own, or appears in a template other than its own macro file; (3) a template holds an inline `<script>` (in any case, with no `src` attribute) other than `<script type="speculationrules">`, an inline event handler attribute (`on…`) or a `javascript:` URL. Askama expressions and blocks (`{{ … }}`, `{% … %}`) are opaque to all three rules' markup parsing. Each rule has a planted-violation fixture the test must refuse.
7. Each R44 component (`grade`, `run_line`, `progress_ray`, `spectrum`, `trace`, `check_row`, `evidence_block`, `diff_block`, `grouping_switch`, `query_input`) exists once as a macro in `templates/components/<name>.html` and once as `static/<name>.css`, typed against its own view struct in `netray-web`; a test renders each from a fixed view and asserts its markup; `static/base.css` holds the marker rule (R43). The spectrum and trace keep the SDD's drawing rules, held by markup tests: a line's width comes from its state's line token (pass `--line-pass`, warn `--line-warn`, fail `--line-fail`), a fixed line is hollow at `--line-fix` with a ring on top, not applicable, not tested and unmeasured lines get the muted mark, a hairline marks each protocol group boundary, group names sit on an axis row below the plate and the warn, fail and fixed lines' short names on a label row above it (§5.4); in the trace a pass row is a hairline and warn and fail rows are blocks (§5.6).
8. `static/logo.svg` is W5b: "net" regular and "ray" bold as SVG `<text>` in the `font.sans` stack, a tick scale under each, the marker line through the scale axis between them. `static/favicon.svg` and `static/badge-mark.svg` are the marker crossing the axis knocked out of a solid ink tile with at most 2 px radius, as on the canvas: one ink-only `fill-rule="evenodd"` path holds the tile and the cut-out axis, ticks and marker (the canvas icon at S = 20, inset 6 px, on the 32 px favicon tile; at S = 11, inset 2.5 px, on the 16 px badge-mark tile). All three are monochrome: their only colours are the `color.light.ink` and `color.dark.ink` token values, no status colour, no gradient. The logo and the favicon are drawn in `color.light.ink` and switch to `color.dark.ink` under an embedded `prefers-color-scheme: dark` rule; the badge mark has no scheme rule and is fixed in both schemes: a `color.light.ink` tile with the cut-out filled in `color.dark.ink`. The logo's `font-family` is the `font.sans` stack. The SVGs are not templates, stylesheets or islands, so gate rule 1 does not scan them; a test compares their literals with the token values instead.
9. `templates/partials/tagline.html` holds the one tagline, "Every check of a domain, in one spectrum."; no other template or stylesheet contains it.

## Phase 1 — Tokens

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5

### Test Scenarios

- GIVEN `crates/web/tokens.json` WHEN the crate builds THEN `tokens::CSS` holds `--color-bg: #f7f8fa` on `:root`, `--color-bg: #07080a` under both dark selectors, and `--font-size-h1: 26px` inside the phone query.
- GIVEN the glow tokens WHEN the CSS is generated THEN `:root` sets `--status-glow-fail: none` and only the dark blocks set its shadow.
- GIVEN a token named `status.light.warn.text` WHEN the CSS is generated THEN its property is `--status-warn-text`.
- GIVEN the real tokens WHEN the contrast test runs THEN it passes; GIVEN a status `text` colour of `#9aa0a6` on the light `bg` THEN it fails naming that colour.
- GIVEN the move WHEN `tests/repo/test_sdd_in_repo.sh` runs THEN it passes with `crates/web/tokens.json` and fails when that file is missing.

## Phase 2 — The gate

**Depends on:** Phase 1
**Requirements:** 6

### Test Scenarios

- GIVEN the crate's real templates and stylesheets WHEN the gate runs THEN it passes.
- GIVEN a planted stylesheet with `color: #fff` THEN the gate fails on rule 1; GIVEN `border-radius: var(--radius-max)` THEN it passes; GIVEN `--r: 50%; border-radius: var(--r)` THEN it fails, `--r` being no token.
- GIVEN a planted stylesheet that defines `.check-row` outside `check_row.css` THEN the gate fails on rule 2.
- GIVEN a planted template with `<script>alert(1)</script>` THEN the gate fails on rule 3; GIVEN `<script type="speculationrules">` THEN it passes.

## Phase 3 — Components

**Depends on:** Phase 2
**Requirements:** 7

### Test Scenarios

- GIVEN a fixed view per component WHEN its macro renders THEN the markup carries the component's root class and the view's values.
- GIVEN the ten components WHEN the gate runs THEN it passes.
- GIVEN a spectrum view with one pass, warn, fail, fixed and unmeasured line in two groups WHEN it renders THEN each line carries its state's width token, the fixed line is hollow with a ring, the unmeasured line is muted, one boundary hairline separates the groups, group names sit below the plate and the warn, fail and fixed short names above it.
- GIVEN a trace view with a pass and a fail row WHEN it renders THEN the pass row is a hairline and the fail row a block.

## Phase 4 — Identity

**Depends on:** Phase 2
**Requirements:** 8, 9

### Test Scenarios

- GIVEN the three SVGs WHEN a test reads them THEN every colour equals `color.light.ink` or `color.dark.ink`, none is a status colour, there is no gradient, no radius over 2 px, and the logo's font stack equals `font.sans`.
- GIVEN the tagline WHEN a test searches templates and stylesheets THEN it appears only in `templates/partials/tagline.html`.

## Decision log

- One feature with four phases over four features (operator, 2026-10-14).
- A library without a route over a preview route (operator, 2026-10-14).
- Component markup, logo and marks from the design canvas (operator, 2026-10-14).
- Logo text as SVG `<text>` in the system stack over outlined paths: R41 loads no web font, and no font file exists to outline from (operator, 2026-10-14).
- Favicon and badge tile at most 2 px radius, over the canvas's 7/4 px or no tile: the SDD (R43, V4.14) wins (operator, 2026-10-14).
- Component macros typed against view structs in `netray-web` over `netray-model` types: 4a stays independent of history and API (operator, 2026-10-14).
- Tagline "Every check of a domain, in one spectrum." (operator, 2026-10-14).
- `netray-web` depends on `netray-common` now, over adding it to the C1 exception list: C1 requires it, and `web`'s rate limits (§3.1) belong to `common`'s rate limiter; §3.1 gains `common` (implementer, after the spec reading; operator confirmed 2026-10-14).
- Gate rule 1 scans templates, stylesheets and islands as §8.5 says, not the SVGs in `static/`; the SVGs carry the ink token values as literals, checked by a test against the tokens: a file fetched as its own image cannot read `tokens.css` (implementer, after the spec reading; operator confirmed 2026-10-14).
- The phone query sits at a new token `layout.phoneMax` = 480px, over `layout.phone` (390 px, the reference width) or raising `layout.phone`: a query at 390 px leaves 393–430 px phones on desktop sizes (operator, 2026-10-14, phase 1 review).
- A `…Phone` token overrides its base property in the phone query, and glow is dark-only (`none` on `:root`), as `v2-tokens.json` describes (implementer, after the spec reading).

- Gate rule 1 binds each property to its own token family, refuses `hwb`/`lab`/`lch`/`oklab`/`color-mix` and system colours, allows CSS-wide keywords and `0`/`none` for widths and radii; rule 2 owns a component's `<root>-…` and `<root>__…` classes; in islands only string literals are scanned, and only for colours, so styling from islands is settled in Phase 4 with the first island (operator, 2026-10-15, phase 2 reading).

- The gate catches honest mistakes, not deliberate evasion: after the second reading it also covers askama expressions in tags, SVG font attributes, `rx`/`ry`, outline and decoration widths, vendor prefixes, deprecated system colours, inline event handlers and `javascript:` URLs; colours URL-encoded inside data URIs, attribute selectors and classes passed through askama strings stay out (operator, 2026-10-15, phase 2 second reading).

- The favicon and badge-mark tile is the canvas knock-out, a solid ink tile with the marks cut out by an ink-only `fill-rule="evenodd"` path and the icon inset as on the canvas, over a 12 % ink tint behind full-ink marks: the tint departs from the canvas board (operator, 2026-10-15, phase 4 reading).
- The badge mark is fixed, `color.dark.ink` marks on a `color.light.ink` tile with no scheme rule, over switching with the viewer's scheme: the badge sits on its own label on third-party pages, where the viewer's scheme says nothing about the ground (1.39:1 on the canvas label) (operator, 2026-10-15, phase 4 reading).
- The badge-mark tile is 16 px with the icon at S = 11, inset 2.5 px, the canvas's 16 px favicon board, over the 12 px icon without a tile of the canvas badge: R8 asks for a tile, and the 16 px board is the canvas's only tile at that size (implementer, phase 4 reading; operator confirmed 2026-10-15).

- `tokens.css` declares `color-scheme` on `:root` (`light dark`) and on `[data-theme="dark"]` (`dark`), over leaving forced-dark pages with light-ink SVGs at 1.07:1 (operator, 2026-10-15, phase 4 reading).

- The spectrum and trace drawing rules of SDD §5.4 and §5.6 join requirement 7 with markup tests now, over leaving them to the pages phase: the phase 3 reader found 13 drawing defects only by rendering (operator, 2026-10-15).

## Open decisions

None.

## Out of scope

- Pages, routes, islands, the rewrite of `frontend-rules.md`: SDD Phase 4 (V4.4–V4.13).
