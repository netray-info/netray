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

## Phase 2 — Layout from catalogue, profiles and runs

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R10: `layout(current, older, catalogue, width)` over per-run statuses, section states and profile; protocol order; shared x; per-run tones, heights, labels, rows, section states; netray-web depends on model and engine (workspace) | green | crates/web/tests/spectrum_runs.rs, tests/repo/test_spectrum_deps.sh |
| C2 | R11: tone mapping, muted for N/A, not tested, unmeasured, absent, unweighted; fixed against the next older run; oldest run never fixed | green | crates/web/tests/spectrum_runs.rs |
| C3 | R12: effective-weight heights normalised to the run's max; muted height = smallest measured; 1.0 with none measured; unweighted check muted, not an error | green | crates/web/tests/spectrum_runs.rs |
| C4 | R13: real-catalogue property test (claims a–f, 54 lines, heights in (0,1], muted ≤ measured, shared x, fixed lines labelled) | green | crates/web/tests/spectrum_catalogue_props.rs |
| C5 | R14: SDD §5.4, §3.9, §8.1, V4.4 and the linePitchMin description amended | green | tests/repo/test_spectrum_deps.sh |
| C6 | real catalogue: line 0 `dns.dnssec`, line 53 `ip.reputation`, boundaries at lines 10, 31, 43, 53 | green | crates/web/tests/spectrum_runs.rs |
| C7 | labels exactly CAA, Expired, HSTS; HSTS fixed | green | crates/web/tests/spectrum_runs.rs |
| C8 | warn → not applicable: muted, unlabelled, not fixed | green | crates/web/tests/spectrum_runs.rs |
| C9 | three runs: oldest has no fixed line; same x per line | green | crates/web/tests/spectrum_runs.rs |
| C10 | `email.bimi` missing: muted at the smallest measured height (> 0) | green | crates/web/tests/spectrum_runs.rs |
| C11 | all pass v3: `ip.reputation` 1.0, `tls.tls_reachable` 0.34, `http.csp` 0.087 | green | crates/web/tests/spectrum_runs.rs |
| C12 | only `tls.tls_reachable` measured in TLS (fail): 1.0, `ip.reputation` 0.286 | green | crates/web/tests/spectrum_runs.rs |
| C13 | `email.dmarc` N/A: muted at the smallest measured height, below 0.27 | green | crates/web/tests/spectrum_runs.rs |
| C14 | all email N/A: every email line muted at the smallest measured height | green | crates/web/tests/spectrum_runs.rs |
| C15 | older run with doubled DNS weight: DNS heights differ, x does not | green | crates/web/tests/spectrum_runs.rs |
| C16 | older profile without `dns.ttl`: succeeds, muted, unlabelled, current not fixed against it | green | crates/web/tests/spectrum_runs.rs |
| C17 | real-catalogue property test holds | green | crates/web/tests/spectrum_catalogue_props.rs |
| C18 | `tests/repo/test_web_crate.sh` passes in its current shape | already_implemented | tests/repo/test_web_crate.sh |

C18 passed before the phase: the guard already holds in its shape (no axum, no `Router`, no `netray` → `netray-web` edge). One test was wrong against R11 and corrected under `ADLC-Test-Change` (r10 expected pass for a pass after a fail).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 3 | sonnet (after a test fix) | 40019 | 76 |
| G2 | 1 | sonnet | 24377 | 20 |
| G1 fix (unscored section) | 1 | sonnet | 24196 | 22 |
| G1 fix (section total) | 1 | sonnet | 29705 | 28 |

### Review

Pass 1:
- AMENDMENT (affected 2, repaired no) a `Measured` section with a weighted unmeasured check drew its answered lines at an effective weight the grade does not give → R12 amended (such a section is `Unmeasured`, its lines muted), SDD §3.9 amended, decision log entry; test r12_unmeasured_check_unscores_its_section.

Pass 2:
- BLOCKER M summed only catalogue lines, so a check an older profile weights but the catalogue dropped was missing from the section total → M from the run's profile and statuses; test r12_section_total_counts_checks_outside_the_catalogue; fixed.

### Behavioural verification

skipped: Phase 2 adds a library function with no route, CLI or UI entry point (spec non-goals).

## Phase 3 — The component renders a layout

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R15: one conversion builds `SpectrumView` from a layout and the shown run; x and height in percent; labels in rows at `scaled_start`'s CSS form; leaders; opaque label boxes after the leaders; band height from the largest row count; scroll box `min-width: calc(<n> * var(--spectrum-line-pitch-min))`; 4a drawing rules hold | green | crates/web/tests/spectrum_component.rs |
| C2 | R16: tokens `spectrum.labelLeader` 8px and `spectrum.labelRow` 14px; labels in `--font-size-label` and `--font-mono`; gate passes | green | crates/web/tests/spectrum_styles.rs |
| C3 | R17: SDD §5.5 point 3 names the row tokens; §8.3 `spectrum` row says it renders a layout with every run's state and one `band-<protocol>` group per protocol | green | tests/repo/test_spectrum_component_sdd.sh |
| C4 | R18: bands per protocol, per-run groups and label sets, `data-shown` on the shown run only, `data-unmeasured` for an Unmeasured section, SDD §5.5 point 4 markup shape | green | crates/web/tests/spectrum_component.rs, tests/repo/test_spectrum_component_sdd.sh |
| C5 | 432 px, fail on line 0: `x1` is `0.9259%`; box `min-width: calc(54 * var(--spectrum-line-pitch-min))` | green | crates/web/tests/spectrum_component.rs |
| C6 | row 2 label: `bottom: calc(var(--spectrum-label-leader) + 2 * var(--spectrum-label-row))`; its leader's height the same calc | green | crates/web/tests/spectrum_component.rs |
| C7 | band height: 4 rows → 4; 1 used row → 3; runs with 3 and 5 rows → 5 | green | crates/web/tests/spectrum_component.rs |
| C8 | groups `band-dns` … `band-ip` in order, each ID once, 10, 21, 12, 10, 1 lines per run | green | crates/web/tests/spectrum_component.rs |
| C9 | three runs: `data-run` 0, 1, 2 per band, three label sets, only the shown run carries `data-shown` | green | crates/web/tests/spectrum_component.rs |
| C10 | email `Unmeasured`: `band-email`'s run group has `data-unmeasured`, lines muted with height > 0; no other band | green | crates/web/tests/spectrum_component.rs |
| C11 | TLS and HTTP `NotApplicable`: no `data-unmeasured`; lines muted with height > 0 | green | crates/web/tests/spectrum_component.rs |
| C12 | two renders with different shown runs differ only in `data-shown` | green | crates/web/tests/spectrum_component.rs |
| C13 | every leader precedes every label in a label set; `.spectrum__line-label` has `background: var(--color-bg)` | green | crates/web/tests/spectrum_component.rs |
| C14 | pass and muted lines: no label and no leader | green | crates/web/tests/spectrum_component.rs |
| C15 | `.spectrum` has `overflow-x: auto`; `.spectrum__line-label` has `font-size: var(--font-size-label)` and `font-family: var(--font-mono)` | green | crates/web/tests/spectrum_styles.rs |
| C16 | `tokens.css` `:root` holds `--spectrum-label-leader: 8px` and `--spectrum-label-row: 14px` | green | crates/web/tests/spectrum_styles.rs |
| C17 | the 4a drawing-rule tests pass against the new view with only their literals changed | green | crates/web/tests/drawing_rules.rs, crates/web/tests/components.rs |
| C18 | the gate passes on the changed template and stylesheet; a planted `bottom: 22px; font-size: 12px` fails on the font size only | green | crates/web/tests/spectrum_styles.rs |

One test was wrong against R11 and corrected under `ADLC-Test-Change` (c7's mixed case expected 3 rows for a current run with two fixed lines). C18's gate plant passed before the phase (the gate never bound `bottom`); kept as a guard. The 4a test helper `line_tag` searches from the plate, since label spans now carry `data-code`.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet (after a test fix) | 47421 | 71 |
| G2 | 1 | sonnet | 26100 | 21 |

### Review

Pass 1: no blockers.
- DEFERRED the IP group name spills past the box's right edge and scrolls the box at 432–920 px → `v2-result-page` (group-name placement and the one-line IP group, spec Out of scope).
- NIT group names moved to `--font-size-label` with the line labels → decision log entry.
- NIT `spectrum_pass_and_unmeasured_lines_carry_no_short_name_above_the_plate` is vacuous since the view carries labels only for placed lines; the rule stays guarded by c14 and property claim (b).
- NIT the scroll box assumes the placement at n × linePitchMin (R15 prose; the caller's width).
- NIT an out-of-range `shown` hides every run.

### Behavioural verification

The reviewer rendered the component with the generated tokens.css and spectrum.css in headless Chromium: 108 lines with 54 visible (only the shown run), labels at 8, 22, 36 and 50 px with leaders ending at their label, band height 8 + rows × 14 px, edge clamp equal to `scaled_start` at 432 and 1056 px. No route serves the component yet (spec non-goals).

### Checks to build

- Every `proptest!` block takes its config from a function a plain test asserts `failure_persistence.is_none()` on (`tests/repo`, see `specs/solutions/2026-10-17-persistence-test-could-not-fail.md`). The fifth note of class `check-passes-without-observing`: a rule is due.
