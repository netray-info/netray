# Plan: v2-spectrum

## Phase 1 — Layout core

## Groups

G1: C1–C8, C10–C21 · G2: C9. No shared production file; C1 and C9 share `tests/repo/test_spectrum_module.sh`, green once both land; one commit (SDD rides with the code).

## Plan

### G1
- `crates/web/src/spectrum.rs` (new): types `LineTone`, `LineInput`, `Group`, `Label`, `Placement`, `LayoutError::NoLines`; `line_pitch_min()` and `label_width()` parse `tokens::values::{SPECTRUM_LINE_PITCH_MIN, FONT_SIZE_LABEL, SPACE_1}` per call (px parse, panics on other units); `place()` computes plate and pitch, xs, groups (maximal runs of equal `group`), boundaries, labels on warn/fail/fixed, box start with edge shift and over-wide at 0, greedy half-open rows (1e-6 tolerance), rows ≥ 3. Must not contain `askama`, `std::fs|io|net|env`, `static mut`, `thread_local!`, `OnceLock`, `LazyLock`, `lazy_static`.
- `crates/web/src/lib.rs`: `pub mod spectrum;` after `pub mod gate;`.

### G2
- `specs/sdd/v2.md` §5.5: point 2 states left alignment at the leader, half-open intervals, the right-edge shift and a box wider than the plate at 0 (`left`, `half-open`, `wider than the plate` between `2. ` and `3. `); point 3 "every non-passing line" → "every warn, fail and fixed line".

## Phase 2 — Layout from catalogue, profiles and runs

## Groups

G1: C1–C4, C6–C17 · G2: C5 (C18 already green). One commit (SDD rides with the code).

## Plan

### G1
- `crates/web/src/spectrum.rs`: `SectionState`, `RunInput`, `LayoutLine`, `RunLayout`, `Layout`; line order by stable sort on `Protocol` over `Catalogue::entries`; `measured(run, spec)` (pass/warn/fail, weighted in `profile.sections[p].checks`, section Measured or absent); base tones per run, then Pass → Fixed when the next older run's base tone is Warn or Fail; heights e = w / M × W_s normalised to the run's max, muted at the smallest measured height, 1.0 when none measured; `layout()` calls `place()` per run (group = protocol index), takes x geometry from run 0. Stays inside the purity allowlist.

### G2
- `specs/sdd/v2.md`: §5.4 first paragraph gives `spectrum::layout(current, older, catalogue, width)` in `netray_web::spectrum` and says a caller maps stored entries onto statuses with profiles; §3.9:454 paragraph adds the measured-only effective weight and the muted height; §8.1 gains the `src/spectrum.rs` row; the V4.4 row names `v2-spectrum`, `v2-result-page` (OG cards incl. `/r/<id>/og.png`, X4) and `v2-inspectors`.
- `crates/web/tokens.json`: `spectrum.linePitchMin.$description` says 54 checks at 390 px with 16 px gutters (358 px of content) give 6.6 px.
