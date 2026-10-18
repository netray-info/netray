# Report: v2-result-page

## Phase 1 — Page views from an entry

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R7: open issues hold every warn and fail once, no other status; row has title, findings or catalogue finding, fix hint, evidence link, guide link | green | crates/web/tests/result_views_issues.rs |
| C2 | R9: issues grouped by fix_owner under §3.5's labels in FixOwner order; severity critical→low, protocol DNS,TLS,HTTP,email,IP; inside a group severity, fail before warn, catalogue order; data-owner/severity/protocol/status/order | green | crates/web/tests/result_views_grouping.rs |
| C3 | R11: diff block rows per CheckChange labelled by class, per BlockChange, grade line with rescored steps, engine note, R19 line without diff | green | crates/web/tests/result_views_diff.rs |
| C4 | R12: after a fresh run Changed/Identical/First/NotStored diff blocks | green | crates/web/tests/result_views_recorded.rs |
| C5 | R13: passed-checks list pass, not_applicable (reason), not_tested, unmeasured ("could not check"); each check exactly once in issues or details | green | crates/web/tests/result_views_passed.rs |
| C6 | R15 (run mapping): run.checks → CheckId→Status, run.sections → SectionState, Profiles::get(profile_version), one function | green | crates/web/tests/result_views_layout.rs |
| C7 | R16: history sentence per line, spans of MM-DD, last as "since"; an issue's since when after the oldest shown entry | green | crates/web/tests/result_views_history.rs |
| C8 | GIVEN two warns and one fail on different owners WHEN issues are built THEN three rows, each once, grouped by owner, fail before warn, each with title, body, fix hint, guide href, evidence href | green | crates/web/tests/result_views_issues.rs |
| C9 | GIVEN a warn with stored findings THEN body is the findings; without THEN catalogue finding | green | crates/web/tests/result_views_issues.rs |
| C10 | GIVEN entry Ab3dE5fG, tls.caa_compliant referencing dns.caa THEN href /r/Ab3dE5fG/dns#ev-dns.caa; without references /r/Ab3dE5fG/<protocol> | green | crates/web/tests/result_views_issue_links.rs |
| C11 | GIVEN a not-stored run with the same check THEN href #ev-dns.caa | green | crates/web/tests/result_views_issue_links.rs |
| C12 | GIVEN pass, not_applicable, not_tested, unmeasured checks THEN none is an issue, each exactly once in passed checks | green | crates/web/tests/result_views_passed.rs |
| C13 | GIVEN a diff holding each of the 14 ChangeClass values THEN one row each with its label | green | crates/web/tests/result_views_diff.rs |
| C14 | GIVEN GradeChange with rescored THEN two steps; without, one | green | crates/web/tests/result_views_diff.rs |
| C15 | GIVEN an entry without a diff THEN "first state recorded — re-check after your change" | green | crates/web/tests/result_views_diff.rs |
| C16 | GIVEN NotStored{diff: None} THEN "not recorded: this run is incomplete" | green | crates/web/tests/result_views_recorded.rs |
| C17 | GIVEN warn,warn,pass,pass over four entries THEN "warn <d1> to <d2> · pass since <d3>" | green | crates/web/tests/result_views_history.rs |
| C18 | GIVEN profile_version 3 THEN statuses carried and profile Profiles::get(3); sections measured/not_applicable/incomplete/timed_out → Measured/NotApplicable/Unmeasured/Unmeasured | green | crates/web/tests/result_views_layout.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 (+1 after a test-side fixture fix) | sonnet | 70383 | 121 |
| G1 review fixes | 1 | sonnet | 35002 | 39 |

The first coder run left one red test whose fixture contradicted itself (`ip.reputation` cycled to pass while five protocol groups were asserted); the fixture was fixed in `298e33a6` under `ADLC-Test-Change`.

### Review

- BLOCKER (fixed): findings joined with a bare space; now "; " as the evidence view joins them.
- BLOCKER (fixed): `recorded_diff_view`'s "unchanged since" and "incomplete, not recorded" notes overwrote the stored diff's own note (engine change, first state); both are now kept, joined with "; ".
- AMENDMENT (repaired in phase, Decision log): a warn or fail whose ID left the catalogue is not an issue and is listed last in the passed-checks details.
- AMENDMENT (repaired in phase, Decision log): a changed block with no items reads its ID alone; one item reads "1 item".
- NIT (fixed with the amendment): "(1 items)".
- NIT (fixed): a one-day span printed its date twice.

### Behavioural verification

skipped: Phase 1 is pure view functions with no entry point; the routes arrive in Phase 2.
