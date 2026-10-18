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

## Phase 2 — Server-rendered pages

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: result::router(ResultState) serves GET /, GET /{domain}, POST /{domain}/run, GET /{domain}/history; non-domain first segments (no dot, root files, .well-known, bimi-logo.svg) are not answered; GET / with d → ?d= handler; lens merges it ahead of spa::handler; every response carries v2-pages' CSP | green | crates/web/tests/result_page_csp.rs, crates/lens/tests/result_mount.rs |
| C2 | R2: segment validated by DomainRuns::validate; refused → 400 not-found template with reason and input; normalised differs → GET 301, POST 400 | green | crates/web/tests/result_page_domain.rs |
| C3 | R3: GET /{domain} renders the latest entry in §5.7 order; no admission, no run, no store write; without entry 200 "not checked yet" + check button | green | crates/web/tests/result_page_render.rs |
| C4 | R4: run line "run of <YYYY-MM-DD HH:MM> UTC · entry <id>"; data-state fresh/stale/none by [cache] ttl_seconds | green | crates/web/tests/result_page_render.rs |
| C5 | R5: share link to /r/<id>; absent without entry and for an incomplete run | green | crates/web/tests/result_page_render.rs |
| C6 | R14 (stored): one element per protocol id="evidence-<protocol>" linking /r/<id>/<protocol> with the section outcome; no block rendered | green | crates/web/tests/result_page_evidence.rs |
| C7 | R15: spectrum and trace over the latest and at most 23 older entries; bands, data-run, data-shown, data-unmeasured; trace rows data-run; figure id="spectrum"; legend pass, warn, fail, fixed, muted | green | crates/web/tests/result_page_spectrum.rs |
| C8 | R18: GET /{domain}/history lists every entry newest first with link, created, last seen, grade, profile version, changes; without entry 404 "not checked yet" with link to /<domain> | green | crates/web/tests/result_page_history.rs |
| C9 | R24: / is the idle page: query_input action / name d, recent list, examples example.com and netray.info, tagline and scope sentence, four /inspect/<tool> links | green | crates/web/tests/result_page_idle.rs |
| C10 | R31: test_header_parity.sh expects for lens's GET / the CSP lens answers for /guide/, without unsafe-inline/unsafe-eval; /health and the rest unchanged | green | tests/repo/test_header_parity.sh |
| C11 | GIVEN a stored entry for shop.example.com WHEN GET THEN header, spectrum, diff, issues, evidence, details in order, run line names the id, store and admission saw no call | green | crates/web/tests/result_page_render.rs |
| C12 | GIVEN last seen 10 min ago THEN stale + autostart visit; 1 min ago fresh, no autostart; none → none, "not checked yet", check button, autostart | green | crates/web/tests/result_page_render.rs |
| C13 | GIVEN a stored entry THEN each evidence-<protocol> links /r/<id>/<protocol>, no evidence_block markup | green | crates/web/tests/result_page_evidence.rs |
| C14 | GIVEN GET /Shop.Example.com. THEN 301 to /shop.example.com; GET /1.2.3.4 THEN 400 with validate's reason | green | crates/web/tests/result_page_domain.rs |
| C15 | GIVEN a stored entry THEN five band groups, one data-run group per shown entry, data-shown only on run 0; older entry with email timed out and tls not applicable → band-email run group data-unmeasured, band-tls not | green | crates/web/tests/result_page_spectrum.rs |
| C16 | GIVEN lens's app WHEN GET /robots.txt, /r/abcdefgh, /api/check/example.com THEN none answered by the result router | green | crates/lens/tests/result_mount.rs |
| C17 | GIVEN 30 entries WHEN GET /shop.example.com/history THEN 30 rows link /r/<id>; the trace shows 24 | green | crates/web/tests/result_page_history.rs |
| C18 | GIVEN GET / THEN idle page with input, examples, tagline, four inspect links; GET /?d=Example.com THEN 301 to /example.com | green | crates/web/tests/result_page_idle.rs |
| C19 | GIVEN any response of the result routes THEN its CSP equals v2-pages' and script-src has no unsafe-inline | green | crates/web/tests/result_page_csp.rs |
| C20 | GIVEN lens's GET / WHEN test_header_parity.sh runs THEN v2-pages' CSP there without unsafe-inline, lens's CSP on /health, passes | green | tests/repo/test_header_parity.sh |
| C21 | GIVEN lens's app WHEN GET /favicon.svg, /favicon.ico, /bimi-logo.svg, /.well-known/mta-sts.txt THEN none answered by the result router | green | crates/lens/tests/result_mount.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 web pages | 3 (one red test was a fixture storing an incomplete run; fixed in b391ba07) | sonnet | 130446 | 275 |
| G2 lens mount | 1 | sonnet | 32685 | 86 |
| review fixes | 3 (one red test looked at the wrong chunk; fixed in 3cbde74b) | sonnet | 45734 | 115 |

`tests/repo/test_smoke_services.sh` compared lens's `/` with its SPA fallback; lens's `/` is the idle page now, so the lens row checks the shell marker (72f8d55d, `ADLC-Test-Change`).

### Review

- BLOCKER (fixed): entries with a profile version this build does not embed were dropped, so the page read "not checked yet"; they are now mapped with `Profiles::current()` (Decision log).
- AMENDMENT (repaired in phase, Decision log): `since` counts an entry lacking the check as its own state.
- NIT (fixed, Decision log): a wrong method on a non-domain segment answered 405; it reaches lens's fallback.
- DEFERRED: `POST /{domain}/run` answers 501 until Phase 3.
- NIT (kept): `GET /localhost.` 301s to `/localhost`, which has no dot and falls to the SPA; `GET /?%64=…` shows the idle page.

### Behavioural verification

Seeded store (`cargo run -p netray-store --example seed`), `netray lens lens.dev.toml` with `LENS_HISTORY__DB_PATH`:

```
GET /                    200, CSP script-src 'self' 'sha256-FBZu…', action="/" name="d", /inspect/{dns,tls,http,ip}
GET /example.com         200, data-state="fresh", entry 7cVwAwQZ, ids result-header spectrum diff issues evidence passed in order
GET /Example.com.        301 → /example.com
GET /example.com/history 200, 1 row linking /r/<id>
GET /does-not-exist      SPA shell (id="root")
```

## Phase 3 — Run route and fragments

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R6: re-check/check form posts trigger to /<domain>/run; without SSE accept runs or joins to the end then 303 /<domain>; refused → 429 Retry-After full page with R21's line | green | crates/web/tests/result_run_form.rs |
| C2 | R19: SSE POST reaches DomainRuns::start(domain, client, trigger, prefetch); fresh visit starts nothing; start/section/end fragments per §5.3; NotStarted 503 no-store | green | crates/web/tests/result_run_stream.rs |
| C3 | R20: every fragment rendered by the page's templates; end fragment HTML equals the page element of the same id | green | crates/web/tests/result_run_equal.rs |
| C4 | R21: refusal answers 429 Retry-After and one run-line fragment with reason and countdown (data-retry-after), then end; four sentences | green | crates/web/tests/result_run_refusal.rs |
| C5 | R22: incomplete run's end fragments: grade incomplete or F, finished sections with unmeasured bands, no share link, live evidence, diff per R12 | green | crates/web/tests/result_run_incomplete.rs |
| C6 | GIVEN stub modules finishing dns, tls, http, email, ip WHEN POSTed with SSE THEN start, five section groups in order, end fragments, end | green | crates/web/tests/result_run_stream.rs |
| C7 | GIVEN a finished run WHEN each end fragment is compared with the page templates rendered for the same run THEN equal; end fragments include #spectrum and the open issues | green | crates/web/tests/result_run_equal.rs |
| C8 | GIVEN any section fragment WHEN parsed in a template THEN its root is figure#spectrum, a ray, issues or evidence element, never an SVG g; finished bands carry data-live | green | crates/web/tests/result_run_stream.rs |
| C9 | GIVEN live tls check not_tested and Done run not_applicable WHEN end fragments arrive THEN spectrum, issues and passed checks show not_applicable | green | crates/web/tests/result_run_equal.rs |
| C10 | GIVEN Changed with a warn referencing dns.caa THEN issue links /r/<new id>/dns#ev-dns.caa as the GET page; NotStored THEN #ev-dns.caa | green | crates/web/tests/result_run_links.rs |
| C11 | GIVEN email section times out THEN its ray segment incomplete, band's current run group data-unmeasured, email evidence the live partial, end header incomplete; with a failed hard fail F | green | crates/web/tests/result_run_incomplete.rs |
| C12 | GIVEN a complete run recorded Changed THEN each evidence element links /r/<new id>/<protocol> | green | crates/web/tests/result_run_links.rs |
| C13 | GIVEN a run in flight with two sections done WHEN a second request joins THEN it first receives those two sections' fragments, then the rest | green | crates/web/tests/result_run_stream.rs |
| C14 | GIVEN a fresh head WHEN trigger=visit THEN start not called, stored end fragments and end; trigger=recheck THEN start called with recheck | green | crates/web/tests/result_run_stream.rs |
| C15 | GIVEN lens's implementation and a run 20 s ago WHEN recheck THEN refused throttle with last_run_ago_s 20; per-IP limit spent THEN ip_limit before admit | green | crates/lens/tests/result_run_admission.rs |
| C16 | GIVEN throttle, last_incomplete, 40 s ago, retry 20 s THEN 429, Retry-After: 20, one fragment "a run 40 s ago was incomplete; a new run is possible in 20 s" | green | crates/web/tests/result_run_refusal.rs |
| C17 | GIVEN budget, ip_limit, global_limit, slots refusals THEN address, address, busy, busy sentences | green | crates/web/tests/result_run_refusal.rs |
| C18 | GIVEN Sec-Purpose: prefetch and NotStarted THEN 503 Cache-Control: no-store | green | crates/web/tests/result_run_stream.rs |
| C19 | GIVEN a form POST without SSE accept WHEN the run completes THEN 303 /<domain>; refusal THEN 429 full page with the line | green | crates/web/tests/result_run_form.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 run route | 1 | sonnet | 162763 | 391 |
| G2 lens | 0 (no production change; the lens test's fixed 40 s / 20 s assumed a complete prior run and a stopped clock, fixed in 15613357) | — | — | — |
| review fixes | 2 | sonnet | 58430 | 101 |

### Review

- AMENDMENT (repaired in phase, Decision log, tests 730d76c0): the end fragments replaced `#result-header`, detaching the stream island's form so `data-stream-state` and `netray:stream-end` landed on a detached node; they now replace `result-grade`, `run-line`, `progress` and `result-share`.
- AMENDMENT (repaired in phase, Decision log): a complete run whose record failed (`Recorded(None)`) showed as incomplete; it keeps its grade and reads "not recorded".
- AMENDMENT (repaired in phase, Decision log): a failed history re-read after a stored record fell back to the not-stored view; the Done run now renders as the recorded entry.

### Behavioural verification

Seeded store, `netray lens` with `LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR=0`:

```
POST /example.com/run (SSE, trigger=recheck)  429, retry-after: 3600, v2-pages CSP, event fragment run-line, end
POST /example.com/run (form, trigger=recheck) 429, full page, "too many checks from your address; try again in …"
POST /example.com/run (SSE, trigger=visit, fresh head)  fragments result-grade run-line progress result-share spectrum diff issues evidence-dns…evidence-ip passed
```
