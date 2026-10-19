# Report: v2-result-fidelity

## Phase 1 — Frame, header and hero

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 Container: at 1280 px header, main and footer content start at x 112 and are 1056 px wide (max-width 1120 px incl. 32 px side padding) | green | crates/web/tests/fidelity_frame_styles.rs |
| C2 | R2 Header: wordmark, tagline beside it, right-aligned nav Inspectors, Guides, API, Status (13 px muted) then the unboxed text button Theme; Inspectors to /inspect/dns, API to /api/v1/docs; Compare moves to the footer; every link apex-relative | green | crates/web/tests/fidelity_header.rs |
| C3 | R3 Result header layout: one row of grade block (letter, score line), run title above the h1, Re-check now and Share right-aligned; at most 100 px tall at 1280 px | green | crates/web/tests/fidelity_result_header.rs |
| C4 | R4 Grade: letter 88 px weight 700 letter-spacing -0.04em line-height 0.85; bar 4 px in the pass colour for a passing grade, warn or fail otherwise | green | crates/web/tests/fidelity_frame_styles.rs |
| C5 | R5 Score line: mono 13 px "score n / 100" from score_run under the entry's profile, n in ink weight 600; descriptor line mono 11 px muted from the one grade legend; absent for an incomplete run and on a grade mismatch (warning names the entry) | green | crates/web/tests/fidelity_score.rs |
| C6 | R6 Run title: mono 11 px uppercase .12em "RUN k · date UTC · c CHECKS · PROFILE v", then duration, CACHED, FIRST RUN; a live run keeps the run line | green | crates/web/tests/fidelity_run_title.rs |
| C7 | R7 h1: the domain is 40 px weight 650 | green | crates/web/tests/fidelity_frame_styles.rs |
| C8 | R8 Buttons: Re-check now primary (44 px, padding 0 18 px, ink background, bg text, 14 px 600, radius 2 px); Share secondary (44 px, 1 px line border, radius 2 px, transparent, ink text); both button elements of one stylesheet | green | crates/web/tests/fidelity_frame_styles.rs |
| C9 | R9 Run duration: the store records each new entry's run duration in ms in a new nullable column; Identical leaves it; older entries carry none; Run, fingerprint and diff unchanged | green | crates/store/tests/duration.rs |
| C10 | R10 Cached marker: CACHED for a stored entry rendered inside the cache window (data-state fresh) and when /domain/run answered from a cached entry | green | crates/web/tests/fidelity_cached.rs |
| C11 | R11 Hard-fail banner between header and spectrum naming failed hard-fail checks and the F rule; muted line naming left-out sections; nothing without either | green | crates/web/tests/fidelity_hard_fail.rs |
| C12 | R12 Heading order: one h1, an h2 per section, group cards h3 and below, no skipped level | green | crates/web/tests/fidelity_headings.rs |
| C13 | R13 Entry page frame: /r/id renders inside the site frame (header, container, footer, shared stylesheet) keeping its title and OG meta | green | crates/web/tests/fidelity_entry_frame.rs |
| C14 | GIVEN a stored entry graded B with overall percentage 81.4 under profile 3 WHEN /example.com renders THEN the grade block holds "score 81 / 100" and the descriptor of B | green | crates/web/tests/fidelity_score.rs |
| C15 | GIVEN an incomplete live run WHEN the header renders THEN there is no score line | green | crates/web/tests/fidelity_score.rs |
| C16 | GIVEN a stored run whose score_run grade differs from its stored grade WHEN the page renders THEN there is no score line and a warning names the entry | green | crates/web/tests/fidelity_score.rs |
| C17 | GIVEN the twelfth entry with 40 checks under profile 3 recorded in 4210 ms WHEN rendered THEN the run title reads "RUN 12 · date UTC · 40 CHECKS · PROFILE 3 · 4.2 S" | green | crates/web/tests/fidelity_run_title.rs |
| C18 | GIVEN an entry recorded before the duration column WHEN rendered THEN the run title has no duration part | green | crates/web/tests/fidelity_run_title.rs |
| C19 | GIVEN a fresh entry inside the cache window WHEN /example.com is fetched THEN the run title ends with CACHED and no run starts | green | crates/web/tests/fidelity_cached.rs |
| C20 | GIVEN a domain's first entry WHEN rendered THEN the run title ends with FIRST RUN | green | crates/web/tests/fidelity_run_title.rs |
| C21 | GIVEN an Identical run of an existing entry WHEN recorded THEN the entry's duration is unchanged | green | crates/store/tests/duration.rs |
| C22 | GIVEN a run with tls.hostname_match failed WHEN rendered THEN the hard-fail banner names "Hostname match" and the grade F | green | crates/web/tests/fidelity_hard_fail.rs |
| C23 | GIVEN a run with email not applicable WHEN rendered THEN the banner's muted line reads "Score computed without: Email (reason)" | green | crates/web/tests/fidelity_hard_fail.rs |
| C24 | GIVEN a run with neither hard fail nor left-out section WHEN rendered THEN no banner element exists | already_implemented | crates/web/tests/fidelity_hard_fail.rs |
| C25 | GIVEN the result page WHEN its headings are listed THEN one h1 and the section headings are h2 with no skipped level | green | crates/web/tests/fidelity_headings.rs |
| C26 | GIVEN /r/id of a stored entry WHEN rendered THEN it carries the site header, the container class and the footer | green | crates/web/tests/fidelity_entry_frame.rs |
| C27 | GIVEN the header WHEN rendered THEN its nav holds Inspectors, Guides, API, Status and the Theme button in that order and no absolute URL | green | crates/web/tests/fidelity_header.rs |
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
