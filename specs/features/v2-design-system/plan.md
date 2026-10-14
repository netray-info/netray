# Plan: v2-design-system

## Phase 1 — Tokens

### Groups

- G1: C2, C10 — the token file moves; SDD and CLAUDE.md name the new path; §3.1 gains `common`.
- G2: C1, C3, C4, C5, C6, C7, C8, C9 — `build.rs`, `src/tokens.rs`, `src/contrast.rs`, `src/lib.rs`, `serde_json` to `[dependencies]`. Shares `lib.rs`, so one group; needs G1's file.

### Plan

- G1: `git mv specs/sdd/v2-tokens.json crates/web/tokens.json` (bytes unchanged, lines 96 and 163 still the plate hairs); `specs/sdd/v2.md` lines 83 (name and Depends-on `common`), 687, 862 (§8.1 row, no old name left), 884 (`crates/web/tokens.json:96`, `:163`); `CLAUDE.md:24` names `crates/web/tokens.json`.
- G2: `build.rs` walks `tokens.json` (skip `$` keys and nodes without `$value`), writes `$OUT_DIR/tokens.css` (`:root` light and theme-less plus glow `none`; dark under `@media (prefers-color-scheme: dark) { :root {…} }` and `[data-theme="dark"]` with the glow shadows; `@media (max-width: <layout.phone>) { :root {…} }` with each `…Phone` overriding its base) and `$OUT_DIR/tokens_values.rs` (UPPER_SNAKE path constants, theme kept, `{:?}`); `src/tokens.rs` includes both; `src/contrast.rs` `ratio` (WCAG 2) and `violations` (mark/hair ≥ 3, text ≥ 4.5 vs bg and plate, both themes, message with path, hex, ground, ratio `{:.2}`); `lib.rs` declares both modules, no `Router` word.
