# Report: v2-spectrum

## Phase 1 — Layout core

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: `netray_web::spectrum` in `crates/web/src/spectrum.rs`, pure (no I/O, askama, global state); the only label placement | green | tests/repo/test_spectrum_module.sh |
| C2 | R2: plate = W and pitch W/n when W ≥ n × linePitchMin, else n × linePitchMin and linePitchMin; min read from tokens; no lines is an error | green | crates/web/tests/spectrum_scenarios.rs |
| C3 | R3: line i at (i + 0.5) × pitch; group start k × pitch, width count × pitch; boundary at each group start but the first | green | crates/web/tests/spectrum_scenarios.rs |
| C4 | R4: box chars × 0.62 × font.size.label + space.1, left-aligned at line, edge shift, over-wide at 0, leader at line x | green | crates/web/tests/spectrum_scenarios.rs |
| C5 | R5: greedy lowest free row in line order, half-open, ≥ 3 rows, new row above, nothing dropped | green | crates/web/tests/spectrum_scenarios.rs |
| C6 | R6: exactly warn, fail and fixed lines carry one label, text = short | green | crates/web/tests/spectrum_scenarios.rs |
| C7 | R7: placement scaled to any W' ≥ W stays valid | green | crates/web/tests/spectrum_layout.rs |
| C8 | R8: property test, 1–120 lines, widths 100–2400, claims (a)–(f), 256 cases, no persistence | green | crates/web/tests/spectrum_layout.rs |
| C9 | R9: SDD §5.5 amended (warn, fail and fixed; left alignment, half-open, over-wide at 0) | green | tests/repo/test_spectrum_module.sh |
| C10 | 54 lines at 1056 px: plate 1056, pitch 19.56, line 0 at 9.78 | green | crates/web/tests/spectrum_scenarios.rs |
| C11 | 54 lines at 432 / 431 / 358 px: plate 432, pitch 8 | green | crates/web/tests/spectrum_scenarios.rs |
| C12 | no lines: error | green | crates/web/tests/spectrum_scenarios.rs |
| C13 | lines 0–3 of 54 failing, 4-char labels at 432 px: rows 0–3, row count 4 | green | crates/web/tests/spectrum_scenarios.rs |
| C14 | same at 648 px: fourth label back in row 0, row count 3 | green | crates/web/tests/spectrum_scenarios.rs |
| C15 | one warn line: row 0, row count 3 | green | crates/web/tests/spectrum_scenarios.rs |
| C16 | last of 54 at 432 px with 8-char label: box at 368.48, leader 428; a fitting box starts at its x | green | crates/web/tests/spectrum_scenarios.rs |
| C17 | 4 lines at 32 px, 8-char fail label: box at 0 | green | crates/web/tests/spectrum_scenarios.rs |
| C18 | pass, muted, fixed: only fixed labelled | green | crates/web/tests/spectrum_scenarios.rs |
| C19 | touching boxes share a row | green | crates/web/tests/spectrum_scenarios.rs |
| C20 | dense placement at 432 scaled to 1056: no overlap, inside plate | green | crates/web/tests/spectrum_scenarios.rs |
| C21 | property test 256 cases, claims hold, no file written under the crate | green | crates/web/tests/spectrum_layout.rs |

One test was wrong against R4 and corrected under `ADLC-Test-Change` (c4 expected an edge shift for a box that fits). C19 and C20 sit in `spectrum_scenarios.rs` beside the scenarios; R8's property test is `spectrum_layout.rs`.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet (after a test fix) | 37147 | 67 |
| G2 | 2 | sonnet | 24745 | 27 |
| G1 fix (scaled_start) | 1 | sonnet | 26309 | 20 |

### Review

Pass 1:
- BLOCKER C21's test could not fail (no persistence check reached) → test asserts the config; fixed.
- BLOCKER purity guard passed grouped imports and statics → widened; fixed.
- AMENDMENT (affected 1, repaired no) an edge-shifted box scaled off its leader → R7 and R8 (g) amended with `scaled_start`, R15 renders it as CSS `clamp()`, SDD §5.5 point 2 amended; decision log entry.

Pass 2:
- BLOCKER purity guard passed print macros, `std::process`, `std::time` → guard is now an allowlist; decision log entry.
- NIT grouped-import regex missed the first item → covered by the allowlist (`names std::{`).
- NIT `scaled_start`'s `max(0)` untested → scenario c7 added.
- DEFERRED the 4a macro still places labels itself → Phase 3 (R15).

### Behavioural verification

skipped: Phase 1 adds a library function with no route, CLI or UI entry point (spec non-goals).
