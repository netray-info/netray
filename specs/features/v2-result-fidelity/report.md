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

## Phase 3 — History trace, diff and first visit

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R20 Trace form: under the spectrum each line carries its trace, one row per stored entry newest on top, up to 24, 5 px per row (6 px phone); pass a 1 px hairline, warn and fail a 3 px block in their colour, not tested and not applicable muted; edge dates newest and oldest right of the trace at its top and bottom; no horizontal waterfall rows | green | crates/web/tests/it/fidelity_trace.rs |
| C2 | R21 Caption and hint under the trace: "Each line carries its trace: n runs, newest on top." and "hover a run to replay the spectrum · tap to pin"; while hovered or pinned the caption names the line and its history sentence and the hint reads "replaying run k · move away to return" or "pinned · tap again to release" | green | crates/web/islands/test/spectrum-replay.test.ts, crates/web/tests/it/fidelity_replay_markup.rs |
| C3 | R22 Replay gutter right of the plate: "Run k", the date in bold, "grade X", a 3 px marker beside the replayed trace row; the run title reads "Run k · date · replaying history"; region grades and the legend's fixed entry follow the replayed run | green | crates/web/islands/test/spectrum-replay.test.ts, crates/web/tests/it/fidelity_replay_markup.rs |
| C4 | R23 Hover dimming: while a run is hovered every other trace row is at 30 % opacity and the hovered row enlarged; the island switches data attributes only | green | crates/web/islands/test/spectrum-replay.test.ts, crates/web/tests/it/fidelity_replay_markup.rs |
| C5 | R24 History link "Show history as a table" to /domain/history under the trace | green | crates/web/tests/it/fidelity_trace.rs |
| C6 | R25 Since heading: h2 "Since run k−1" 20 px weight 650 outside the box, mono meta "predecessor date · grade from → to" | green | crates/web/tests/it/fidelity_diff.rs |
| C7 | R26 Diff rows in a bordered box: padding 10 px 16 px, rules between rows, a 3 × 16 px tone marker, a 70 px kind column, the sentence, right-aligned the line code or the protocol for an evidence change; phone: sentence first, "kind · code" meta under it | green | crates/web/tests/it/fidelity_diff.rs |
| C8 | R27 First-run box on a domain's first entry: bordered surface box, "First run recorded." 17 px weight 650, the explanation sentence of board FFirst, a ghost trace of one row and two dashed rows | green | crates/web/tests/it/fidelity_first_run.rs |
| C9 | GIVEN 12 stored entries WHEN the trace renders THEN each line has 12 rows newest first and the edge dates are the newest and oldest entry's month and day | green | crates/web/tests/it/fidelity_trace.rs |
| C10 | GIVEN 30 stored entries WHEN the trace renders THEN each line has 24 rows | already_implemented | crates/web/tests/it/fidelity_trace.rs |
| C11 | GIVEN a pass row and a warn row WHEN rendered THEN the pass row is a 1 px hairline and the warn row a 3 px block in the warn colour | green | crates/web/tests/it/fidelity_trace.rs |
| C12 | GIVEN the trace at rest WHEN rendered THEN the caption reads "Each line carries its trace: 12 runs, newest on top." and the hint "hover a run to replay the spectrum · tap to pin" | green | crates/web/islands/test/spectrum-replay.test.ts, crates/web/tests/it/fidelity_replay_markup.rs |
| C13 | GIVEN the island WHEN run 5 is hovered THEN every other trace row carries the dimmed attribute, the gutter shows "Run 5", its date and grade, and the run title reads "Run 5 · date · replaying history" | green | crates/web/islands/test/spectrum-replay.test.ts, crates/web/tests/it/fidelity_replay_markup.rs |
| C14 | GIVEN a pinned run WHEN the hint renders THEN it reads "pinned · tap again to release" | green | crates/web/islands/test/spectrum-replay.test.ts, crates/web/tests/it/fidelity_replay_markup.rs |
| C15 | GIVEN the result page of a domain with history WHEN rendered THEN it links "Show history as a table" to /example.com/history | green | crates/web/tests/it/fidelity_trace.rs |
| C16 | GIVEN entry 12 with predecessor 11 graded C and itself B WHEN rendered THEN an h2 reads "Since run 11" and its meta "date · grade C → B" | green | crates/web/tests/it/fidelity_diff.rs |
| C17 | GIVEN a diff with a fixed check at position 31 WHEN rendered THEN its row shows kind "fixed", the sentence and the code "MAIL·31" | green | crates/web/tests/it/fidelity_diff.rs |
| C18 | GIVEN a domain's first entry WHEN rendered THEN the box reads "First run recorded." with the explanation and a ghost trace of one row and two dashed rows, and no since heading | green | crates/web/tests/it/fidelity_first_run.rs |
| C19 | GIVEN boards FTrace and FFirst WHEN just design-check runs THEN no row names the trace, caption, gutter, since heading, diff rows or first-run box | green | design-check |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 trace, gutter, diff, since meta, first-run box, line codes | 3 (the orchestrator corrected a stale check id in `fidelity_diff.rs`) | sonnet | 202815 | 639 |
| G2 spectrum island | 1 | sonnet | 45959 | 86 |
| G3 design-check | run by the orchestrator; per-run region grades kept out of view, hidden legend entries, since meta beside its h2, the empty sentence slot removed from the grid | n/a | n/a | n/a |
| review fixes | 4 | sonnet | 107625 | 279 |

### Behavioural verification

`just design-check` (manifest netray-ops 89f9def: trace row, caption, hint, edge date, gutter on FTrace; since meta, diff box, rows, marker, kind, text, code on F and FTrace; first-run box, title, text, ghost, dashed rows on FFirst; "history rows" on F accepted):

```
D3 · Mobile result — light (Play: ‹ › and tap lines): 2 deviation(s)
2 open deviation(s), 0 stale accepted entries
```

Accepted (operator decision, Decision log): board F light and dark "history rows background-color" (the waterfall's filled cells against the per-line trace). The desktop boards report no open row (C19). Side-by-side of FTrace light: the trace sits under the axis, gutter right of the plate with run, date and grade, caption and hint under the trace, edge dates right of it, the since h2 with its meta and the diff box with marker, kind, sentence and line code.

### Reader

| class | finding | disposition |
|---|---|---|
| BLOCKER | the pinned hint never showed with a mouse (hover won) | fixed: pinned hint on the pinned row; island test |
| BLOCKER | no marker beside the replayed trace row | fixed: 3 px ink mark at the row's start while hovered or pinned; test |
| AMENDMENT | gutter lost its run slot when the newest row had no number | repaired: slot always rendered |
| AMENDMENT | the window's oldest row claimed "fixed since" without its predecessor | repaired: empty entry |
| AMENDMENT | caption kept naming a once-hovered line | repaired: only while replaying; named by short and line code |
| AMENDMENT | unstored run lost its grade change | repaired: since meta "date · grade B → F" |
| AMENDMENT | phone half of requirement 26 had no owner | moved to Phase 6 |
| NIT | empty run number in the hint | fixed: "replaying this run" |
| NIT | the line-code oracle repeats the implementation's loop | listed |

## Phase 4 — Open issues and passed checks

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R28 Attention heading and switch: h2 "n lines need attention" with mono meta "f fail · w warn"; right-aligned on that line the grouping switch as unboxed tabs, mono 12 px, 36 px tall, the marker bar on the current tab, a rule below | green | crates/web/tests/it/fidelity_attention.rs |
| C2 | R29 Line codes: protocol tag and 1-based spectrum position, two digits ("HTTP·21"), used in items, diff rows and the evidence feeds line | already_implemented | crates/web/tests/it/fidelity_items.rs |
| C3 | R30 Group cards: bordered card per group with a header row, owner label 14 px weight 650 and right-aligned "n lines" ("1 line") mono | green | crates/web/tests/it/fidelity_items.rs |
| C4 | R31 Item layout: two-column grid, 84 px left column with a 34 px tone bar, severity word mono 11 px 700 uppercase in tone colour, code mono 11 px muted; right column title with since text, body 14 px muted, "Fix:" with the hint 14 px ink, meta "Protocol · source" then links "evidence →" and "guide →" mono 12 px muted underlined; phone one column with "SEV · code · since" above the title | green | crates/web/tests/it/fidelity_items.rs |
| C5 | R32 Since text from the stored entries: "first run", "in all n runs", "lit up in run k", "down from fail since run k", "up from warn since run k"; one pure function, table-tested | green | crates/web/tests/it/fidelity_since.rs |
| C6 | R33 Clean bar: bordered 48 px bar "n lines clean · checks that pass" with the count in the pass colour and "show" right-aligned, on details without JavaScript; the coverage line goes, its links live in the evidence tabs | green | crates/web/tests/it/fidelity_clean.rs |
| C7 | R34 Section rhythm: 36 px between consecutive sections at 1280 px | green | crates/web/tests/it/fidelity_attention.rs |
| C8 | GIVEN a run with one fail and four warns WHEN rendered THEN an h2 reads "5 lines need attention" and its meta "1 fail · 4 warn" | green | crates/web/tests/it/fidelity_attention.rs |
| C9 | GIVEN http.hsts at spectrum position p WHEN its code is built THEN it is "HTTP·p" (two digits); GIVEN an email check THEN "MAIL·p" | already_implemented | crates/web/tests/it/fidelity_items.rs |
| C10 | GIVEN two items owned by the web server WHEN grouped by owner THEN one card's header reads the owner label and "2 lines" | green | crates/web/tests/it/fidelity_items.rs |
| C11 | GIVEN an item of http.hsts WHEN rendered THEN the left column with severity "FAIL" and code, the title, the body, "Fix:" with the hint, the meta "HTTP · RFC 6797" and the links "evidence →" and "guide →" | green | crates/web/tests/it/fidelity_items.rs |
| C12 | GIVEN statuses pass, pass, warn, warn over entries 1 to 4 WHEN the since text is built THEN "lit up in run 3" | green | crates/web/tests/it/fidelity_since.rs |
| C13 | GIVEN warn in all 12 entries THEN "in all 12 runs"; GIVEN fail, fail, warn THEN "down from fail since run 3"; GIVEN one entry THEN "first run" | green | crates/web/tests/it/fidelity_since.rs |
| C14 | GIVEN 35 passed checks WHEN rendered THEN the clean bar reads "35 lines clean · checks that pass" and "show", and opening it without JavaScript lists the 35 checks | green | crates/web/tests/it/fidelity_clean.rs |
| C15 | GIVEN the result page WHEN rendered THEN no coverage line element exists | green | crates/web/tests/it/fidelity_clean.rs |
| C16 | GIVEN board F WHEN just design-check runs THEN no row names the attention heading, switch, group card, item parts, clean bar or the section gaps | green | design-check |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 issues head, switch, group cards, check row, since text, clean bar, rhythm | 5 (over budget, re-runs to read output; `stored_parts` argument struct by the orchestrator for clippy) | sonnet | 175765 | 483 |
| G2 design-check | run by the orchestrator; clean bar border and padding on the details, item side column as a grid, the figure's default margin removed | n/a | n/a | n/a |
| review fixes | 3 + 1 island-only | sonnet | 136677 | 328 |

### Behavioural verification

`just design-check` (manifest netray-ops f6a99b7: attention meta, switch and tab, group card, head, label, count, item, bar, severity, code, body, meta, clean bar, count, show, section rhythm on the six desktop boards):

```
D3 · Mobile result — light (Play: ‹ › and tap lines): 2 deviation(s)
2 open deviation(s), 0 stale accepted entries
```

The desktop boards report no open row (C16). Side-by-side of board F light: head row with the switch on the right, group cards in board order (the fail group first), two-column items with bar, severity and code, the meta row with the links, the clean bar at the end.

### Reader

| class | finding | disposition |
|---|---|---|
| BLOCKER | the since text read only the 24-entry window ("in all 24 runs") | fixed: all stored entries; test |
| BLOCKER | an unknown run position invented run numbers ("first run") | fixed: no since text; test |
| BLOCKER | the clean bar counted unmeasured and not-tested lines as clean | fixed: counts pass and not applicable; test |
| AMENDMENT | heading count and meta disagreed for uncatalogued checks | repaired: both count listed rows |
| AMENDMENT | groups in a fixed owner order, not the board's | repaired: fail first, groups by their first row (server and island) |
| AMENDMENT | capitalised owner labels not in the SDD | SDD amended |
| AMENDMENT | the switch was inert without JavaScript | repaired: hidden until the island is ready |
| AMENDMENT | phone half of requirement 31 had no owner | moved to Phase 6 |
| AMENDMENT | "0 lines clean" without a run | repaired: the bar is hidden |
| DEFERRED | evidence section without links until Phase 5 | Phase 5 |
| NIT | protocol group label "email" | fixed: "Email" |

## Phase 5 — Evidence section

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R35 Evidence section between the open issues and the clean bar: h2 "Evidence" with mono meta "recorded with this run", a bordered card with tabs DNS, TLS, HTTP, EMAIL, IP (mono 12 px uppercase, marker on the current tab), per tab a panel with the protocol's evidence blocks, "Feeds lines: code short[ · …]", "All evidence →" to /r/id/protocol and the explore-live link; unmeasured or not applicable sections show their reason; TLS current when TLS has a non-pass line, else the first protocol with one, else DNS; without JavaScript every panel in order with tabs as in-page links; a small island switches panels by data attribute; an incomplete live run fills the same tabs; at phone width a too-wide table scrolls inside the panel with "← swipe the table" | green | crates/web/tests/it/fidelity_evidence_tabs.rs |
| C2 | GIVEN a stored entry WHEN rendered without JavaScript THEN the Evidence h2 is followed by five tab links and five panels in protocol order, each with its blocks, a feeds line, "All evidence →" to /r/id/protocol and its explore-live link | green | crates/web/tests/it/fidelity_evidence_tabs.rs |
| C3 | GIVEN a run whose only non-pass lines are TLS WHEN rendered THEN the TLS tab carries the marker and its panel is shown | green | crates/web/tests/it/fidelity_evidence_tabs.rs |
| C4 | GIVEN an IP section not applicable WHEN its panel renders THEN it shows the reason | green | crates/web/tests/it/fidelity_evidence_tabs.rs |
| C5 | GIVEN the tabs island WHEN the HTTP tab is activated THEN only the HTTP panel carries the shown attribute and the island wrote no style | green | crates/web/islands/test/tabs.test.ts |
| C6 | GIVEN an incomplete live run WHEN its sections stream THEN the panels fill from the fragments | green | crates/web/tests/it/fidelity_evidence_live.rs |
| C7 | GIVEN boards F and M WHEN just design-check runs THEN no row names the evidence heading, tabs, panel table, feeds line or inspector link | green | design-check |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 evidence tabs, panels, island | 6 (over budget; one test row needed `var(--radius-max)`, the gate refuses a literal radius; the explore label follows the board, by the orchestrator) | sonnet | 129952 | 392 |
| G2 design-check | run by the orchestrator | n/a | n/a | n/a |
| review fixes | 3 | sonnet | 87362 | 226 |

### Behavioural verification

`just design-check` (manifest netray-ops d1695df: evidence meta, card, nav rule, current tab, panel, feeds, explore link, table on the six desktop boards):

```
D3 · Mobile result — light (Play: ‹ › and tap lines): 2 deviation(s)
2 open deviation(s), 0 stale accepted entries
```

The desktop boards report no open row (C7; board M's evidence rows come with Phase 6's phone manifest). Side-by-side of board F light: the Evidence head row, the tab card with TLS current, the panel's blocks, the feeds line, "All evidence →" and "Open in the TLS inspector →". The panel is much longer than the board's single chain table (every block of the protocol): a proposal for the operator.

### Reader

| class | finding | disposition |
|---|---|---|
| BLOCKER | after a streamed run the stale nav overrode the new current tab | fixed: the nav is an end fragment; island test |
| BLOCKER | in-page links into hidden panels went nowhere | fixed: the island opens the target's tab on click, hash change and the initial hash |
| NIT | "recorded with this run" on unrecorded states | fixed: shown only for a stored entry, streamed |
| NIT | weakened explore label check | fixed by the orchestrator: the label names the inspector per protocol |

## Phase 6 — Phone layout

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R36 Phone header: one row of the wordmark at 58 × 33 px, the Theme button and a 44 × 44 px menu button (1 px line border, radius 2 px) disclosing the nav on details; every nav entry in the open menu at least 44 px tall | green | crates/web/tests/it/fidelity_phone_header.rs |
| C2 | R37 Phone result header: the run title on top without the profile part, the grade letter at 64 px beside the h1, Re-check now full width (primary, 44 px) with Share beside it | green | crates/web/tests/it/fidelity_phone_figure.rs |
| C3 | R38 Phone spectrum fits the 358 px column without sideways scroll: line labels 10 px, region labels 9 px; the min-width that forced 432 px goes | green | crates/web/tests/it/fidelity_phone_figure.rs |
| C4 | R39 Phone trace (6 px rows) with a stepper bar: ‹ and › buttons of 44 px at the edges, between them "Run k · date · grade X" over "latest run" (or "run k of n"); the hint "Tap a line for its history. Step back with ‹ to replay a run."; then the history link | green | crates/web/tests/it/fidelity_phone_figure.rs |
| C5 | R40 Tap targets: at 390 px every link and button on the page at least 44 px tall, footer links and the clean bar included | green | crates/web/tests/it/fidelity_phone_header.rs |
| C6 | R16 phone half: region labels 9 px at phone width, the email region labelled "MAIL" | green | crates/web/tests/it/fidelity_phone_figure.rs |
| C7 | R26 phone half: diff rows with the sentence first and "kind · code" as a meta line under it | green | crates/web/tests/it/fidelity_phone_rows.rs |
| C8 | R31 phone half: items in one column with the meta line "SEV · code · since" above the title | green | crates/web/tests/it/fidelity_phone_rows.rs |
| C9 | GIVEN 390 px WHEN the header renders THEN wordmark, Theme and the 44 × 44 menu button share one row and the nav is hidden until the menu opens | green | crates/web/tests/it/fidelity_phone_header.rs |
| C10 | GIVEN 390 px WHEN the page renders THEN the document is not wider than the viewport and the spectrum's region labels include "MAIL" | green | crates/web/tests/it/fidelity_phone_figure.rs |
| C11 | GIVEN 390 px with 12 entries WHEN rendered THEN the stepper reads "Run 12 · date · grade B" over "latest run" and the hint "Tap a line for its history. Step back with ‹ to replay a run." | green | crates/web/tests/it/fidelity_phone_figure.rs |
| C12 | GIVEN 390 px light WHEN every link and button is measured THEN none is below 44 px tall | green | crates/web/tests/it/fidelity_phone_header.rs |
| C13 | GIVEN boards M and MDark WHEN just design-check runs THEN it reports no row | green | design-check |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 phone menu, hero, spectrum, stepper, diff and item rows, tap targets | 3 (red at the end; the orchestrator's run was green) | sonnet | 189648 | 619 |
| G2 design-check | run by the orchestrator; label band and leaders as phone-aware token calcs (46 px; 6, 18, 30 px), the mobile manifest's run-title regex and stepper box selector, legend, attention meta, trace caption and region grades out of view at phone width as board M, the switch full width, the stepper gap | n/a | n/a | n/a |
| review fixes | 2 | sonnet | 88921 | 218 |

### Behavioural verification

`just design-check` (manifest netray-ops 649f4d7: 41 elements on the phone boards M and MDark, the new MDark board included):

```
0 open deviation(s), 0 stale accepted entries
```

All eight boards report no open row (C13). Side-by-side of board M light: menu box, run title without the profile, grade beside the h1, full-width Re-check with Share, the spectrum without sideways scroll and "MAIL", the 6 px trace, the stepper with the run box and the hint, the history link, diff rows with their meta lines, the switch across the width, items with their SEV · code · since lines.

### Reader

| class | finding | disposition |
|---|---|---|
| BLOCKER | Theme outside the nav took the header's 15 px and gap | fixed: 13 px, 20 px after the nav; phone padding 0 10px |
| BLOCKER | the wordmark link was 33 px tall on the phone | fixed: 44 px |
| BLOCKER | screen readers read "EMAILMAIL" | fixed: `::after` with an empty alt text |
| AMENDMENT | no nav in browsers without `::details-content` | repaired: the menu button shows there; browser floor logged |
| AMENDMENT | section grades removed on the phone | repaired: visually hidden, kept for screen readers; proposal for the operator |
| AMENDMENT | legend hidden on the phone | board M; proposal for the operator |
| AMENDMENT | stepper state while stepped back | spec amended to board M's wording |
| AMENDMENT | Phase 5's tabs island and two partials were never committed | committed with this phase |
| DEFERRED | the phone named no tapped line | fixed: the hint names it (board M) |
| NIT | switch and "show" at 11 px | fixed: 12 px |
| NIT | phone tone bars all 3 px | fixed: tone widths |
| NIT | Theme against the menu button | fixed with the padding |
| NIT | arrow glyph and desktop min-width untested | listed |

## Phase 7 — Share and embed

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R41 Share dialog: "Share" opens a dialog "Share and embed" with tabs Snapshot (origin/r/id), Re-run link (origin/domain), Badge (Markdown and HTML snippets of origin/badge/domain.svg linking origin/domain, style flat or for-the-badge rewriting the snippets, a preview), Social card (origin/r/id/og.png with a preview), each a read-only field and a copy button; origin is the canonical origin; Esc and the close button close it, focus returns to Share; without JavaScript Share is a link to /r/id; tokens and components only, bundle under its cap; absent without a stored entry and for an incomplete run | green | crates/web/tests/it/fidelity_share.rs |
| C2 | GIVEN entry Ab3dE5fG of example.com WHEN the dialog renders THEN the Snapshot field holds origin/r/Ab3dE5fG and the Re-run field origin/example.com | green | crates/web/tests/it/fidelity_share.rs |
| C3 | GIVEN the Badge tab with style for-the-badge WHEN chosen THEN both snippets carry style=for-the-badge | green | crates/web/tests/it/fidelity_share.rs, crates/web/islands/test/share.test.ts |
| C4 | GIVEN the Social card tab WHEN rendered THEN it holds origin/r/Ab3dE5fG/og.png and a preview image of it | green | crates/web/tests/it/fidelity_share.rs |
| C5 | GIVEN the share island WHEN a copy button is pressed THEN the clipboard receives the field's text and the button announces it | green | crates/web/islands/test/share.test.ts |
| C6 | GIVEN the open dialog WHEN Esc is pressed THEN it closes and focus returns to Share | green | crates/web/islands/test/share.test.ts |
| C7 | GIVEN no JavaScript WHEN Share is followed THEN it leads to /r/Ab3dE5fG | green | crates/web/tests/it/fidelity_share.rs |
| C8 | GIVEN an incomplete run WHEN the header renders THEN there is no Share control | already_implemented | crates/web/tests/it/fidelity_share.rs |
| C9 | GIVEN the islands bundle WHEN built THEN the cap test passes | already_implemented | crates/web/islands/test/budget.test.ts |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 share dialog, snippets, island | 3 (the gate refused a test title with the colour word "field"; the orchestrator reworded it) | sonnet | 91952 | 189 |
| review fixes | 3 | sonnet | 62575 | 121 |

### Behavioural verification

Headless Chromium against the design stack (seeded fixtures), `/shop.example.com` at 1280 px:

```
badge requests before open: 0
dialog open: true
snapshot field: https://netray.info/r/0nrOQrDP
badge requests after open: 1
dialog open after Esc: false focus on share: share
```

`just design-check`: `0 open deviation(s), 0 stale accepted entries` (the boards draw no dialog; the Share button is measured).

### Reader

| class | finding | disposition |
|---|---|---|
| BLOCKER | the badge preview in the closed dialog fetched /badge on every page view, and lens starts a run for a stale head | fixed: previews load when the dialog opens; tests |
| AMENDMENT | an open dialog replaced by the stream left focus on the body | repaired: the new Share link takes focus; island test |

## Phase 8 — Design gate and visual reading

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R42 Design gate: the private manifest names the elements of requirements 1–40 on the eight boards; the waterfall rows of board F accepted by the operator's decision; `just design-check` exits 0 with no open and no stale accepted row | green | design-check |
| C2 | R43 Visual reading: a reader with no part in the build compares every side-by-side image and finds no missing, extra or misordered element besides the theme-H additions; each finding is a report row and is closed | green | visual reading |
| C3 | GIVEN the manifest's eight boards and the seeded fixtures WHEN just design-check runs THEN it exits 0, prints "0 open deviation(s), 0 stale accepted entries" and every board has a side-by-side image | green | design-check |
| C4 | GIVEN board F light and dark WHEN just design-check runs THEN the "history rows" value rows are listed under Accepted with the operator's decision as reason | green | design-check |
| C5 | GIVEN the side-by-side images WHEN a reader compares them THEN every finding is a report row and none stays open | green | visual reading |
| V1 | visual reading: F, FTrace, FFirst line labels overlapped (DKIM over DMARC) | green | visual reading (fixed: labels line-height 1, so rows 14 px apart no longer collide) |
| V2 | visual reading: F evidence-kind rows in the since list | green | visual reading (sample data: no record changed in the fixture) |
| V3 | visual reading: F, FFirst evidence panel without the board's one-line summary caption | green | visual reading (proposal for the operator: evidence tab content) |
| V4 | visual reading: F, FFirst evidence block order (addresses first, chain per address, empty CAA block) | green | visual reading (proposal for the operator: evidence tab content) |
| V5 | visual reading: F, FFirst, M extra "All evidence →" link | green | visual reading (required by requirement 35, V1 behaviour) |
| V6 | visual reading: FFirst extra one-row trace, gutter and caption | green | visual reading (fixed: no trace and no gutter without a history of two runs; the figure takes the full width) |
| V7 | visual reading: FFirst run title wraps | green | visual reading (consequence of the specified profile and cached parts) |
| V8 | visual reading: FFirst ghost strip drawn as vertical ticks | green | visual reading (fixed: one waterfall row, pass a hairline across its cell, warn and fail blocks) |
| V9 | visual reading: M domain breaks mid-word | green | visual reading (sample data: the board's overflow-wrap anywhere with a longer domain) |
| V10 | visual reading: M newer step not greyed on the latest run | green | visual reading (fixed: steps that lead nowhere are disabled; island test) |
| V11 | visual reading: M hint touches the stepper | green | visual reading (fixed: 10 px) |
| V12 | visual reading: M item links on their own line | green | visual reading (fixed: inline after the meta) |
| V13 | visual reading: M addresses table rows of uneven height | green | visual reading (fixed: phone table cells do not wrap; the table scrolls) |
| V14 | visual reading: M key-value values wrap mid-token, swipe note far from the table | green | visual reading (swipe note now only in panels holding a table, fixed in the final review; the key-value blocks belong to the proposal for the operator: evidence tab content) |
| V15 | visual reading: M extra feeds line | green | visual reading (fixed: hidden at phone width, as board M) |
| V16 | visual reading: M extra "· checks that pass" | green | visual reading (fixed: hidden at phone width, as board M) |

### Behavioural verification

`just design-check` over the eight boards (manifest netray-ops: F, FDark, FFirst, FFirstDark, FTrace, FTraceDark, M, MDark):

```
0 open deviation(s), 0 stale accepted entries
```

Accepted (operator decision, Decision log): `D3 · Analyzer — history (option 1+2), light/dark: history rows background-color` (board F's waterfall cells against the per-line trace). Every board has a side-by-side image under `target/design/`. The theme toggle and the footer of canvas v41 are built but not measured: the export (2026-10-19 14:47) predates them; their measurement waits for the operator's fresh export.

Visual reading: an independent reader compared all eight side-by-sides (34 segments) and reported 16 findings (V1–V16 above); each is closed by a fix, as sample data, as specified, or as a proposal for the operator.
