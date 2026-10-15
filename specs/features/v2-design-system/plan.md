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

## Phase 4 — Identity

### Groups

- G1: C4, C6 — `crates/web/templates/partials/tagline.html`
- G2: C1, C2, C3, C5 — `crates/web/static/logo.svg`, `static/favicon.svg`, `static/badge-mark.svg` (C3/C5 read all three, so one group)

### Plan

**G1.** `templates/partials/tagline.html`: exactly `Every check of a domain, in one spectrum.` and a newline; no markup. Plain text, so the gate's real-tree scan passes.

**G2.** Shared: colours only in one `<style>`, through element or class selectors (no `#id` selector); `#0f1216` (`color.light.ink`) only outside, `#e7e9ec` (`color.dark.ink`) only inside `@media (prefers-color-scheme: dark)`; no `url(`, `gradient`, colour function or other `#`; geometry as `<rect>` with plain numbers.

- `static/logo.svg`, em = 100, `viewBox="0 0 351 160"`, `<title>netray</title>`. Style: `text { font-family: <FONT_SANS>; font-size: 100px; letter-spacing: -0.03em }`, `text, rect { fill: #0f1216 }`, dark `fill: #e7e9ec`. `<text x="0" y="93" font-weight="400" textLength="160" lengthAdjust="spacingAndGlyphs">net</text>`, `<text x="176" y="93" font-weight="700" textLength="175" lengthAdjust="spacingAndGlyphs">ray</text>` (`textLength` pins each word to its scale in any system font). `<g id="scale-net">`: axis `x=0 y=118 w=160 h=3.5`, 6 ticks `w=3 y=121.5` at x 0/31.4/62.8/94.2/125.6/157, heights 24/10/10/10/10/24. `<g id="scale-ray">`: axis `x=160 y=118 w=191 h=3.5`, 5 ticks at x 176/219/262/305/348, heights 10/10/10/10/24. Marker `<rect id="marker" x="164.5" y="0" width="7.5" height="160"/>`, last.
- `static/favicon.svg` (amended 2026-10-15): `viewBox="0 0 32 32" width="32" height="32"`, one `<path id="tile" fill-rule="evenodd">`: the 32 px tile with `A2 2` corners, then the outline of the union of the canvas icon at S = 20, inset 6 (axis `6 19.2 20×1.4`, ticks at x 6.6/10.4/22 `1.2×3.2`, marker `14.6 7.2 2.8×17.6`); `path { fill: #0f1216 }`, dark `#e7e9ec`.
- `static/badge-mark.svg` (amended 2026-10-15): `viewBox="0 0 16 16"`, the same knock-out at S = 11, inset 2.5 (axis `2.5 9.76 11×1`, ticks at x 2.83/4.92/11.3 `1×1.76`, marker `7 3.16 2×9.68`) as `<path id="tile" class="tile">`, plus `<path id="mark" class="mark">` with the union outline; `.tile { fill: #0f1216 }`, `.mark { fill: #e7e9ec }`, no scheme rule.
