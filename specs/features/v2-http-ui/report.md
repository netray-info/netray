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
