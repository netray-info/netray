# Report: v2-result-fidelity

## Phase 1 — Frame, header and hero

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 Container: at 1280 px header, main and footer content start at x 112 and are 1056 px wide (max-width 1120 px incl. 32 px side padding) | green | crates/web/tests/it/fidelity_frame_styles.rs |
| C2 | R2 Header: wordmark, tagline beside it, right-aligned nav Inspectors, Guides, API, Status (13 px muted) then the unboxed text button Theme; Inspectors to /inspect/dns, API to /api/v1/docs; Compare moves to the footer; every link apex-relative | green | crates/web/tests/it/fidelity_header.rs |
| C3 | R3 Result header layout: one row of grade block (letter, score line), run title above the h1, Re-check now and Share right-aligned; at most 100 px tall at 1280 px | green | crates/web/tests/it/fidelity_result_header.rs |
| C4 | R4 Grade: letter 88 px weight 700 letter-spacing -0.04em line-height 0.85; bar 4 px in the pass colour for a passing grade, warn or fail otherwise | green | crates/web/tests/it/fidelity_frame_styles.rs |
| C5 | R5 Score line: mono 13 px "score n / 100" from score_run under the entry's profile, n in ink weight 600; descriptor line mono 11 px muted from the one grade legend; absent for an incomplete run and on a grade mismatch (warning names the entry) | green | crates/web/tests/it/fidelity_score.rs |
| C6 | R6 Run title: mono 11 px uppercase .12em "RUN k · date UTC · c CHECKS · PROFILE v", then duration, CACHED, FIRST RUN; a live run keeps the run line | green | crates/web/tests/it/fidelity_run_title.rs |
| C7 | R7 h1: the domain is 40 px weight 650 | green | crates/web/tests/it/fidelity_frame_styles.rs |
| C8 | R8 Buttons: Re-check now primary (44 px, padding 0 18 px, ink background, bg text, 14 px 600, radius 2 px); Share secondary (44 px, 1 px line border, radius 2 px, transparent, ink text); both button elements of one stylesheet | green | crates/web/tests/it/fidelity_frame_styles.rs |
| C9 | R9 Run duration: the store records each new entry's run duration in ms in a new nullable column; Identical leaves it; older entries carry none; Run, fingerprint and diff unchanged | green | crates/store/tests/it/duration.rs |
| C10 | R10 Cached marker: CACHED for a stored entry rendered inside the cache window (data-state fresh) and when /domain/run answered from a cached entry | green | crates/web/tests/it/fidelity_cached.rs |
| C11 | R11 Hard-fail banner between header and spectrum naming failed hard-fail checks and the F rule; muted line naming left-out sections; nothing without either | green | crates/web/tests/it/fidelity_hard_fail.rs |
| C12 | R12 Heading order: one h1, an h2 per section, group cards h3 and below, no skipped level | green | crates/web/tests/it/fidelity_headings.rs |
| C13 | R13 Entry page frame: /r/id renders inside the site frame (header, container, footer, shared stylesheet) keeping its title and OG meta | green | crates/web/tests/it/fidelity_entry_frame.rs |
| C14 | GIVEN a stored entry graded B with overall percentage 81.4 under profile 3 WHEN /example.com renders THEN the grade block holds "score 81 / 100" and the descriptor of B | green | crates/web/tests/it/fidelity_score.rs |
| C15 | GIVEN an incomplete live run WHEN the header renders THEN there is no score line | green | crates/web/tests/it/fidelity_score.rs |
| C16 | GIVEN a stored run whose score_run grade differs from its stored grade WHEN the page renders THEN there is no score line and a warning names the entry | green | crates/web/tests/it/fidelity_score.rs |
| C17 | GIVEN the twelfth entry with 40 checks under profile 3 recorded in 4210 ms WHEN rendered THEN the run title reads "RUN 12 · date UTC · 40 CHECKS · PROFILE 3 · 4.2 S" | green | crates/web/tests/it/fidelity_run_title.rs |
| C18 | GIVEN an entry recorded before the duration column WHEN rendered THEN the run title has no duration part | green | crates/web/tests/it/fidelity_run_title.rs |
| C19 | GIVEN a fresh entry inside the cache window WHEN /example.com is fetched THEN the run title ends with CACHED and no run starts | green | crates/web/tests/it/fidelity_cached.rs |
| C20 | GIVEN a domain's first entry WHEN rendered THEN the run title ends with FIRST RUN | green | crates/web/tests/it/fidelity_run_title.rs |
| C21 | GIVEN an Identical run of an existing entry WHEN recorded THEN the entry's duration is unchanged | green | crates/store/tests/it/duration.rs |
| C22 | GIVEN a run with tls.hostname_match failed WHEN rendered THEN the hard-fail banner names "Hostname match" and the grade F | green | crates/web/tests/it/fidelity_hard_fail.rs |
| C23 | GIVEN a run with email not applicable WHEN rendered THEN the banner's muted line reads "Score computed without: Email (reason)" | green | crates/web/tests/it/fidelity_hard_fail.rs |
| C24 | GIVEN a run with neither hard fail nor left-out section WHEN rendered THEN no banner element exists | already_implemented | crates/web/tests/it/fidelity_hard_fail.rs |
| C25 | GIVEN the result page WHEN its headings are listed THEN one h1 and the section headings are h2 with no skipped level | green | crates/web/tests/it/fidelity_headings.rs |
| C26 | GIVEN /r/id of a stored entry WHEN rendered THEN it carries the site header, the container class and the footer | green | crates/web/tests/it/fidelity_entry_frame.rs |
| C27 | GIVEN the header WHEN rendered THEN its nav holds Inspectors, Guides, API, Status and the Theme button in that order and no absolute URL | green | crates/web/tests/it/fidelity_header.rs |
| C28 | GIVEN the result page at 1280 px light and dark WHEN just design-check runs THEN no row names the header, grade, score, run title, h1, Re-check or Share elements | green | design-check |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 frame, header, stylesheets | 2 | sonnet | 69637 | 139 |
| G2 store, lens, score, run title, banner, headings | 3 (green after the orchestrator added `button` to `entries_gate.rs`) | sonnet | 178156 | 1954 |
| G3 entry page in the frame | 3 | sonnet | 58743 | 1145 |
| G4 design-check | run by the orchestrator; four stylesheet fixes and consistent grades in the shop fixtures | n/a | n/a | n/a |
| review fixes | 3 | sonnet | 98265 | 1524 |

### Behavioural verification

`just design-check` (manifest netray-ops 5862b57 plus phase 1 elements, export of 2026-10-19 14:47) after G1–G3 and the stylesheet fixes:

```
D3 · Mobile result — light (Play: ‹ › and tap lines): 2 deviation(s)
2 open deviation(s), 0 stale accepted entries
```

The six 1280 px boards (F, FDark, FFirst, FFirstDark, FTrace, FTraceDark) report no row for container, header nav, tagline, hero, grade bar, grade letter, score line and number, run title, h1, Re-check and Share buttons and section heading (C28). The two open rows are the phone board's h1 line-height and score font size, owned by Phase 6. The shop fixtures' stored grades disagreed with `score_run` (D, C, B stored; B, B, A scored), so the score guard hid the score: their grades are now B, B, B (one more warn in the newest entry), which keeps the page honest and the board measurable. The theme toggle and the footer of canvas v41 are built but not measured: the export predates them and the operator's fresh export is awaited.

### Reader

| class | finding | disposition |
|---|---|---|
| BLOCKER | a history re-read failure after a record made the title "Run 1 · first run" and the diff "First run recorded." | fixed: the ordinal is unknown there; title without run number and first-run part, heading "Since the previous run"; tests in `fidelity_review_p1.rs` |
| BLOCKER | a grade mismatch with an incomplete score hid the score without the warning | fixed: the warning fires on every mismatch |
| AMENDMENT | the hard-fail sentence showed under a letter it contradicts | repaired in phase: the sentence follows the score guard; spec and SDD amended |
| AMENDMENT | unstored and unrecorded runs never showed the banner | repaired in phase: scored under their own profile |
| AMENDMENT | the run number renumbers after the retention sweep | spec and SDD state it (stored entries are counted) |
| AMENDMENT | the history migration is forward-only | SDD §4.1 states it |
| NIT | `open_count` counts checks the catalogue dropped | listed; not reachable today (catalogue_emitters) |
| NIT | an unrecorded-run assertion that cannot fail | listed; the fallback has its own tests now |

## Phase 2 — Spectrum figure

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R14 Line heights: a measured line's height is plate × (0.35 + 0.65 × w / w_max), w its effective weight, w_max the run's largest; unmeasured and not-applicable lines keep their muted mark; SDD §5.4 amended | green | crates/web/tests/it/fidelity_heights.rs |
| C2 | R15 Plate and axis: plate radius 2 px; a 32 px axis with N + 1 ticks, 4 px, 10 px at the plate's ends and region bounds | green | crates/web/tests/it/fidelity_axis.rs |
| C3 | R16 Region labels: mono 11 px uppercase .14em centred under each region (phone 9 px, email "MAIL"); each carries the section's letter grade from score_run's section percentage with the profile's thresholds; none without a score | green | crates/web/tests/it/fidelity_regions.rs |
| C4 | R17 Line labels mono 11 px weight 600 on a transparent background; label band 56 px for up to three rows with leaders in three staggered lengths, growing only for a fourth row | green | crates/web/tests/it/fidelity_legend.rs |
| C5 | R18 Legend: pass, warn, fail as 2, 3, 4 px marks, "fixed since run k−1" as the hollow fix line with its ring, mono 12 px, no "muted" entry; key note "N checks · line height = weight in the grade" mono 12 px muted | green | crates/web/tests/it/fidelity_legend.rs |
| C6 | R19 Expiry label: the catalogue short of tls.expiry_window is "Expiry" | green | crates/web/tests/it/fidelity_regions.rs |
| C7 | GIVEN effective weights 10, 5 and 2 WHEN laid out on a 92 px plate THEN the heights are 92, 62.1 and 44.2 px within rounding | green | crates/web/tests/it/fidelity_heights.rs |
| C8 | GIVEN a run with one unmeasured section WHEN laid out THEN that section's lines keep the muted mark and do not change w_max | green | crates/web/tests/it/fidelity_heights.rs |
| C9 | GIVEN 40 lines in five regions WHEN rendered THEN the axis holds 41 ticks, the ends and the four region bounds 10 px and the others 4 px | green | crates/web/tests/it/fidelity_axis.rs |
| C10 | GIVEN a DNS section at 84 percent under profile 3's thresholds WHEN rendered THEN its region label reads "DNS" followed by that letter | green | crates/web/tests/it/fidelity_regions.rs |
| C11 | GIVEN a not-applicable email section WHEN rendered THEN its region label reads "EMAIL" with no letter | green | crates/web/tests/it/fidelity_regions.rs |
| C12 | GIVEN the legend WHEN rendered THEN pass, warn, fail and fixed entries each with its mark element, no "muted" entry, and the key note "N checks · line height = weight in the grade" | green | crates/web/tests/it/fidelity_legend.rs |
| C13 | GIVEN the catalogue WHEN tls.expiry_window is read THEN its short is "Expiry" | green | crates/web/tests/it/fidelity_regions.rs |
| C14 | GIVEN boards F and M WHEN just design-check runs THEN no row names the plate, axis, ticks, region labels, line labels, label band or legend | green | design-check |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 spectrum layout, component, legend, catalogue | 3 (two test assertions corrected by the orchestrator: label `data-code` stays the check id; leaders follow rows per S20) | sonnet | 150181 | 731 |
| G2 design-check | run by the orchestrator; legend spacing and fix ring, design fixtures to the boards' story | n/a | n/a | n/a |
| review fixes | 4 | sonnet | 106442 | 422 |

### Behavioural verification

`just design-check` (manifest netray-ops 9ba1cc7: phase 2 elements plate, axis, ticks, region label, line label, label band, legend, legend mark, key note on the six desktop boards):

```
D3 · Mobile result — light (Play: ‹ › and tap lines): 2 deviation(s)
2 open deviation(s), 0 stale accepted entries
```

The desktop boards report no row (C14); the two phone rows belong to Phase 6. The side-by-side of board F light shows the plate, the axis with ticks and graded region labels, the 56 px label band and the legend row with the fix ring in the board's arrangement.

### Reader

| class | finding | disposition |
|---|---|---|
| BLOCKER | the last tick at 100 % made the spectrum scroll sideways by 1 px | fixed: `data-last` tick ends at the plate edge; test |
| BLOCKER | the one-line IP region's label ran past the plate | fixed: labels centre with symmetric overflow, the last end-aligned; test |
| BLOCKER | the phone half of requirement 16 (9 px, MAIL) was not built | amended: moved to Phase 6 with requirement 38 |
| AMENDMENT | region grades and legend name the latest run during replay | requirement 22 extended; built with Phase 3 |
| AMENDMENT | region grades skipped the score guard | repaired: hidden on a grade mismatch; test |
| AMENDMENT | key note counted the run's checks ("0 checks" while streaming) | repaired: counts the lines drawn; absent without a figure |
| AMENDMENT | label widths measured at 12 px while drawn at 11 px | repaired: measured at the caption size; SDD §5.5 amended |
| AMENDMENT | leader stagger by index (board) versus by row (S20) | decision logged; requirement 17 reworded |
| AMENDMENT | first-visit legend without a fixed entry | as board FFirst; logged |
| DEFERRED | design fixtures' evidence blocks contradict their statuses | Phase 5 (evidence tabs) |
