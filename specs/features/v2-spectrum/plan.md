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
