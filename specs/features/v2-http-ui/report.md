# Report: v2-http-ui

## Phase 1 — Protocol views and value display

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 protocol views in pages/views asked per block by every renderer; evidence.rs names no module | green | crates/web/tests/it/views_claim.rs, views_same_markup.rs |
| C2 | R2 a view fragment carries id ev-block and the feeds line | green | crates/web/tests/it/views_claim.rs |
| C3 | R3 a view claims a block only when it renders every key; other key sets fall back | green | crates/web/tests/it/views_claim.rs |
| C4 | R4 the inspector re-streams parts when the band brings checks; same markup in all three places | green | crates/web/tests/it/views_same_markup.rs, views_live_restream.rs |
| C5 | R5 facts_grid component, macro and stylesheet, cells with tone and chips | green | crates/web/tests/it/facts_grid.rs |
| C6 | R6 display values for durations, bools, times; format_value stays raw | green | crates/web/tests/it/evidence_display_values.rs |
| C7 | R7 repeat suffix shown as the key, full key in data-key and exports | green | crates/web/tests/it/evidence_repeat_suffix.rs |
| C8 | S a claimed http.hsts renders once on /r/id/http as the fragment with id and feeds line | green | crates/web/tests/it/views_claim.rs |
| C9 | S the shared inspector run, the evidence view and the live details render the same fragment | green | crates/web/tests/it/views_same_markup.rs |
| C10 | S a live HTTP run re-sends the part after the band event with the check tone | green | crates/web/tests/it/views_live_restream.rs |
| C11 | S evidence.rs names no block prefix | already_implemented | crates/web/tests/it/evidence_render_source.rs |
| C12 | S a stored http.hsts with an unknown key renders through the generic block | green | crates/web/tests/it/views_claim.rs |
| C13 | S durations read 77 ms, 1.2 s, 2 min 5 s | green | crates/web/tests/it/evidence_display_values.rs |
| C14 | S response_ms 77 reads response and 77 ms in the generic block | green | crates/web/tests/it/evidence_display_values.rs |
| C15 | S a time reads 2024-01-01 00:00 UTC inside a time element with datetime | green | crates/web/tests/it/evidence_display_values.rs |
| C16 | S a bool true reads yes in the generic block | green | crates/web/tests/it/evidence_display_values.rs |
| C17 | S set-cookie and set-cookie#2 key cells read set-cookie, the second with data-key | green | crates/web/tests/it/evidence_repeat_suffix.rs |
| C18 | S JSON and CSV exports keep set-cookie#2 and raw values | already_implemented | crates/web/tests/it/evidence_repeat_suffix.rs |
| C19 | S facts_grid renders root class, labels, values, data-tone and its stylesheet | green | crates/web/tests/it/facts_grid.rs |

C11 was already in place (`evidence_render_source.rs`, v2-inspectors); C18 passed on the baseline:
the exports never read the display formatter.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 + G2 (one test binary) | 2 | opus | 160768 | 532 |

### Review

| class | finding | disposition |
|---|---|---|
| BLOCKER | a time before 1970 read 1970-01-01 00:00 UTC (shared minute formatter clamps) | fixed: civil date computed in `display_time`; test `times_before_1970_and_with_an_offset_read_in_utc` |
| AMENDMENT | the repeat suffix renamed lone literal names such as `x-trace#1` | requirement 7 amended: strip only when the block holds the base key; test `a_lone_key_with_a_digit_suffix_is_a_literal_name` |
| AMENDMENT | enrichment badges exist only in the inspector, so "same markup" cannot hold for addresses | requirement 4 amended |
| DEFERRED | the view title is a 15 px h3 inside the card, not G's 20 px h2 above it | Phase 2 and the Phase 4 reading |

### Behavioural verification

Local lens from this worktree, history seeded from `tests/acceptance/fixtures/history`
(scratchpad `lens-up.sh`, port 18431). `curl -s http://127.0.0.1:18431/r/18uljF5v/http`
(shop.example.com, first entry, which holds `http.hsts`):

```
<section class="protocol-view" id="ev-http.hsts" data-block="http.hsts"><h3 class="protocol-view__title">Strict-Transport-Security</h3>
<div class="facts-grid"><div class="facts-grid__cell" data-tone="pass"><span class="facts-grid__label">max-age</span><span class="facts-grid__value">1 year (31536000 s)</span></div><div class="facts-grid__cell" data-tone="pass"><span class="facts-grid__label">includeSubDomains</span><span class="facts-grid__value">no</span></div><div class="facts-grid__cell" data-tone="pass"><span class="facts-grid__label">preload</span><span class="facts-grid__value">no</span></div></div>
<p class="protocol-view__feeds">Feeds lines: HSTS</p>
</section>
```

The generic blocks on the same page read `<dt>redirects_to_https</dt><dd>yes` (was `true`).

## Phase 2 — CSP, HSTS and server views

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8 CSP view: mode, count, root issues, directive table with sources and toned issues, scroll container | green | crates/web/tests/it/views_http_csp.rs |
| C2 | R9 HSTS view: units, not parsed, yes/no, tone from the http.hsts check | already_implemented | crates/web/tests/it/views_http_hsts.rs |
| C3 | R10 server view: facts grid with links, Inspection took line, no tone; response_ms not claimed | green | crates/web/tests/it/views_http_server.rs |
| C4 | R15 views need no JavaScript and add no island | already_implemented | crates/web/tests/it/views_frame_style.rs |
| C5 | S enforced CSP reads Enforced, 2 issues, the base-uri root issue, rows in order, script-src sources and issue | green | crates/web/tests/it/views_http_csp.rs |
| C6 | S report-only reads Report-Only with data-tone warn and the sentence | green | crates/web/tests/it/views_http_csp.rs |
| C7 | S a 4 kB policy: each source its own element, the table in a horizontal scroll container | green | crates/web/tests/it/views_http_csp.rs |
| C8 | S no CSP header: no CSP view | already_implemented | crates/web/tests/it/views_http_csp.rs |
| C9 | S max-age 31536000 with includeSubDomains and a passing check reads 1 year, yes, no, all pass | already_implemented | crates/web/tests/it/views_http_hsts.rs |
| C10 | S http.hsts without max_age reads not parsed | already_implemented | crates/web/tests/it/views_http_hsts.rs |
| C11 | S http.server with inspection_ms 1234 shows URL, status, version and Inspection took 1.2 s | green | crates/web/tests/it/views_http_server.rs |
| C12 | S the server view's address keeps the shell's link | green | crates/web/tests/it/views_http_server.rs |
| C13 | S nothing lost for CSP, HSTS and server fixtures | green | crates/web/tests/it/views_nothing_lost.rs |
| C14 | Phase 1 review deferral: the view title is G's section heading (20 px, 650, -0.01em) above the card | green | crates/web/tests/it/views_frame_style.rs |

C2, C9, C10 passed on the baseline (the HSTS view came with Phase 1); C4 and C8 hold because no
view ships a script and no CSP block means no view.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | opus, then three test expectations corrected by the orchestrator | 77901 | 253 |

The three corrected expectations (ADLC-Test-Change): the title's font size is the token
`var(--font-size-h2)` (the CSS gate refuses literal sizes); askama 0.16 escapes `'` as `&#39;`
and `&<>"` as numeric entities, so `views_http_csp.rs` and `views_nothing_lost.rs` expect that form.

### Review

No findings (reviewer: CSP, server, HSTS, frame and stylesheet traced against the backend blocks).

### Behavioural verification

Local lens from this worktree, seeded history; `curl -s http://127.0.0.1:18431/r/<first shop.example.com entry>/http`:

```
<section class="protocol-view" id="ev-http.server" data-block="http.server"><h3 class="protocol-view__title">Server</h3>
<div class="protocol-view__card">
<div class="facts-grid"><div class="facts-grid__cell"><span class="facts-grid__label">Final URL</span><span class="facts-grid__value">https://shop.example.com/</span></div>…<div class="facts-grid__cell"><span class="facts-grid__label">Address</span><span class="facts-grid__value"><a class="evidence-value__link" href="/inspect/ip?ip=198.51.100.10">198.51.100.10</a></span></div>…</div>
<p class="protocol-view__note">Inspection took 289 ms</p></div>
</section>
```

The fixtures hold no `http.csp` block; the CSP view is pinned by `views_http_csp.rs`.

## Phase 3 — Caching, CDN and isolation views

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R11 caching view: directives as chips, max-age, ETag with weak, Last-Modified, Vary, Age, not set, empty line | green | crates/web/tests/it/views_http_caching.rs |
| C2 | R12 CDN view: provider, cache status, indicators as chips; none without the block | green | crates/web/tests/it/views_http_cdn.rs |
| C3 | R13 isolation view: COOP with the http.coop tone, COEP and CORP present or missing without tone, the note | green | crates/web/tests/it/views_http_isolation.rs |
| C4 | R16 exports keep stored keys and raw values | already_implemented | crates/web/tests/it/views_http_export.rs |
| C5 | S the full caching example reads chips, 1 h (3600 s), W/"abc" weak, 2024-01-01 00:00 UTC, two Vary chips, 42 s | green | crates/web/tests/it/views_http_caching.rs |
| C6 | S caching with only an ETag reads not set for Last-Modified, Vary and Age | green | crates/web/tests/it/views_http_caching.rs |
| C7 | S an empty http.caching reads No caching headers | green | crates/web/tests/it/views_http_caching.rs |
| C8 | S CDN with provider, cache status and two indicators shows all; no block no view | green | crates/web/tests/it/views_http_cdn.rs |
| C9 | S isolation with coop same-origin, two missing and a passing check reads pass, missing, missing and the note | green | crates/web/tests/it/views_http_isolation.rs |
| C10 | S isolation with corp same-site reads same-site and present | green | crates/web/tests/it/views_http_isolation.rs |
| C11 | S nothing lost for caching, CDN and isolation fixtures | green | crates/web/tests/it/views_nothing_lost.rs |
| C12 | S the JSON export keeps max_age 3600 and etag_weak true | already_implemented | crates/web/tests/it/views_http_export.rs |


C4 and C12 passed on the baseline: the exports never read the views.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 50295 | 87 |

### Review

| class | finding | disposition |
|---|---|---|
| BLOCKER | Age printed plain seconds, not whole units as requirement 9 | fixed: `seconds()`; test `age_reads_in_the_largest_whole_unit_and_max_age_alone_proves_cache_control` |
| BLOCKER | Cache-Control read "not set" beside a recorded max-age | fixed: a `max-age=<n>` chip; same test |
| AMENDMENT | the block records six directives only, so "not set" and "No caching headers" overclaim | requirement 11 amended: "none recorded", "No caching headers recorded"; backend follow-up proposed |
| AMENDMENT | the backend stores directives in a fixed order, not the received one | requirement 11 amended: stored order |
| DEFERRED | an unreadable COOP/COEP/CORP value is stored as missing | backend follow-up proposal in the Decision log |
| NIT | an empty COEP/CORP value reads ", present" | kept: the backend stores the empty value as received |
| NIT | protected tests changed | trailers on the phase commit |

The nothing-lost fixture for `http.isolation` held every header both received and missing, which
the backend never writes; it is two consistent rows now, and such a block is refused by the view.

### Behavioural verification

Local lens from this worktree, seeded history; `curl -s http://127.0.0.1:18431/shop.example.com/http`
(before the review fixes; Age and Cache-Control changed after):

```
<h3 class="protocol-view__title">Caching</h3> … Cache-Control: public · max-age: 5 min (300 s) · ETag: not set · Last-Modified: <time datetime="2026-10-13T20:08:28Z">2026-10-13 20:08 UTC</time> · Vary: not set · Age: 9 s … Feeds lines: Caching
<h3 class="protocol-view__title">CDN</h3> … Provider: Generic CDN · Cache status: HIT · Indicators: x-cache
<h3 class="protocol-view__title">Cross-origin isolation</h3> … COOP same-origin (data-tone="pass") · COEP missing · CORP missing · "COEP and CORP are not part of the grade." … Feeds lines: COOP
```

## Phase 4 — Slim band, manifest rows and design gate

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R14 the inspector band renders the plate only: no label band, no axis, no region word; 96 px plate | green | crates/web/tests/it/inspect_band_slim.rs |
| C2 | R17 the manifest holds the descriptor and All evidence rows | green | design-check |
| C3 | R18 design-check exits 0 and the reader's findings beside board G are closed | green | design-check, reading below |
| C4 | S a finished inspector band holds the plate and line names and no label band, axis or region label | green | crates/web/tests/it/inspect_band_slim.rs |
| C5 | S the result page's spectrum keeps its label band and axis | already_implemented | crates/web/tests/it/inspect_band_slim.rs |
| C6 | S the slim plate is 96 px tall | green | crates/web/tests/it/inspect_band_slim.rs |
| C7 | S design-check exits 0 with the descriptor and All evidence rows measured | green | design-check |
| C8 | S every finding of the reading beside board G is a report row and none stays open | green | reading below |
| C9 | S /inspect/http at 390 px with the 4 kB policy is not wider than the viewport | green | measured on the evidence view `/csp.example.com/http` (8 kB policy, 390 px wide) and on `/inspect/http` (390 px wide); a live HTTP run is blocked locally, and the inspector renders the same view fragment (`views_same_markup.rs`) |

### Visual reading beside board G

A reader with no part in the build compared the HTTP inspector (`/inspect/http?url=https://1.1.1.1/`; the probes timed out locally, so only the port-80 probe and the server view render) and the evidence view of a seeded fixture holding every HTTP view (`/csp.example.com/http`, a policy of 8 kB) at 1280 and 390 px, light and dark, with boards 17 and 18.

| id | finding | status | disposition |
|---|---|---|---|
| V1 | inspector band: the h1 sits on the plate (G leaves about 22 px) | green | rebuilt: `.inspector-band` margin-top `--space-5` |
| V2 | inspector band: G's two guide rules in the plate are missing | green | rebuilt: `spectrum__guide` lines in the slim plate; test `the_slim_plate_draws_board_gs_two_guide_rules` |
| V3 | views: a heading sits 8 to 15 px under the card before it (G about 45 px) | green | rebuilt: `.protocol-view` margin-block-start `--space-9`; test `the_feeds_line_sits_inside_the_card_and_views_keep_the_section_gap` |
| V4 | views: the feeds line stands outside the card, flush against the next heading | green | rebuilt: inside the card at its foot; same test |
| V5 | facts grid: a toned cell is indented against untoned ones (isolation) | green | rebuilt: in a grid with a toned cell every cell keeps the bar slot (`data-toned`); an untoned grid stays flush; test `only_a_grid_with_a_toned_cell_reserves_the_bar_slot` |
| V6 | facts grid at 390: label to value distance differs between cells | green | kept: the address is a link, and the shell gives links a 44 px tap target at phone width (v2-inspector-shell phase 9) |
| V7 | inspector band at 390 runs past the gutter | green | kept: the band scrolls inside its figure (`overflow-x: auto`), as G wraps its figure; the document is 390 px wide (C9) |
| V8 | band labels alternate on two rows (G: one row) | green | kept: v2-inspector-shell phase 9 decision (two rows so names never overlap) |
| V9 | check list without a card frame; one continuous tone line at 390 | green | not this feature: the shell's live check list (v2-inspector-shell); proposal for the operator |
| V10 | tab row "?" and the export links where G has the back link | green | not this feature: v2-inspector-shell decisions (help, exports; back link needs DNS, outside the manifest) |
| V11 | evidence view: no band, run line below the actions, "?" box | green | not this feature: the evidence view frame (v2-inspectors, v2-inspector-shell); proposal for the operator |
| V12 | generic blocks (redirects, port 80, headers, CORS, cookies) as key/value rows stacked in one card | green | not this feature: the generic evidence block; their protocol views are later work (proposal) |
| V13 | section order follows the run's block order, not line order | green | kept: SDD §6.1 orders blocks as the run holds them |
| V14 | the address value is underlined | green | kept: the shell's link to the IP inspector (v2-inspector-shell requirement 19) |

### Behavioural verification

`NETRAY_DESIGN_DIR=/Users/lukas/Documents/src/netray.info/specs/design/canvas just design-check`:
`0 open deviation(s), 0 stale accepted entries`; 12 boards, 865 elements, 1614 properties
measured (2 accepted, unchanged). New rows measured: grade descriptor 11 px mono (F family),
10 px (M, MDark), colour; "All evidence →" 13 px (F family, M, MDark); band plate height 96 px
(G, GDark).

Document width (Playwright, scratchpad `shots.mjs`), every page and theme:

```
evidence 1280 light: scrollWidth 1280 viewport 1280
evidence 390 light: scrollWidth 390 viewport 390
inspector 1280 light: scrollWidth 1280 viewport 1280
inspector 390 light: scrollWidth 390 viewport 390
(dark identical)
```

Phase 4 review: SDD §5.4 amended for the inspector band (AMENDMENT); the V6 fix changed nothing
at 390 px and was reverted, the row's cause is the 44 px tap target (AMENDMENT); the bar slot made
untoned grids indent against the card's note and feeds lines, so only a toned grid reserves it
(AMENDMENT).

The CSP table of 124 sources wraps between source elements inside its card at 390 px.

### Final review

Spec reader majors, both held and fixed after the reading: a duration of 59 950 to 59 999 ms read
"1 min 0 s" (now "59.9 s", boundary case in `evidence_display_values.rs`); C9 claimed the 4 kB
policy on `/inspect/http`, which a timed-out local run never showed (re-scoped above, the
limitation logged). Reader minors: the nothing-lost needles for `issue_count` and the HSTS flags
could not fail; they now read "2 issues" and distinct yes/no values.

