# Report: v2-inspector-shell

## Phase 1 — Shell frame and form

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 One shell: one inspector_shell macro and one shell island build every inspector page; per-tool view struct; no template branch on a tool name | green | tests/it/shell_frame.rs |
| C2 | R2 Inspector header: name at 21 px weight 650 and tagline after the wordmark; nav marks Inspectors current; no in-page 40 px h1 | green | tests/it/shell_header.rs |
| C3 | R3 Inspector tabs DNS TLS HTTP EMAIL IP with marker and target carry-over, IDNA hosts in xn-- form | green | tests/it/shell_tabs.rs |
| C4 | R4 Query form card: label, 44 px mono input, clear button, Inspect submit | green | tests/it/shell_form.rs |
| C5 | R5 Example chips: links to the prefilled page; with JS fill and submit | green | tests/it/shell_examples.rs, islands/test/shell-clear.test.ts |
| C6 | R6 Run title LIVE date UTC NOT RECORDED, h1 target, back link with grade letter read from the store | green | tests/it/shell_run_head.rs |
| C7 | R7 Section headings: sections are cards with h2, one h1, idle h1 visually hidden | green | tests/it/shell_headings.rs |
| C8 | R26 No JavaScript: header, tabs, form, example links and the JS notice render | green | tests/it/shell_nojs.rs |
| C9 | GIVEN /inspect/tls WHEN rendered THEN header TLS inspector and TLS, illuminated, Inspectors current, no 40 px h1 | green | tests/it/shell_header.rs |
| C10 | GIVEN /inspect/tls?h=example.com:8443 THEN tabs DNS ?q=example.com, HTTP ?url=https://example.com/, EMAIL ?d=example.com, TLS marked | green | tests/it/shell_tabs.rs |
| C11 | GIVEN /inspect/http?url=https://www.example.com/path THEN TLS tab ?h=www.example.com:443 and EMAIL ?d=example.com | green | tests/it/shell_tabs.rs |
| C12 | GIVEN /inspect/dns?q=bücher.example.com THEN TLS tab ?h=xn--bcher-kva.example.com:443 | green | tests/it/shell_tabs.rs |
| C13 | GIVEN /inspect/ip?ip=192.0.2.24 THEN DNS, TLS, HTTP tabs bare and EMAIL /inspect/email | green | tests/it/shell_tabs.rs |
| C14 | GIVEN /inspect/dns THEN no template branch on the tool name and the DNS builder in the view slot | green | tests/it/shell_frame.rs |
| C15 | GIVEN an input with text WHEN clear is activated THEN the input is empty and focused | green | islands/test/shell-clear.test.ts |
| C16 | GIVEN the idle TLS page without JS WHEN a chip is followed THEN it leads to /inspect/tls?h=example | green | tests/it/shell_examples.rs |
| C17 | GIVEN a finished TLS run of example.com:443 THEN run title, h1 example.com:443, back link to /example.com | green | tests/it/shell_run_head.rs |
| C18 | GIVEN a stored entry graded B THEN the back link starts with B; GIVEN none THEN no letter | green | tests/it/shell_run_head.rs |
| C19 | GIVEN a TLS run against a store WHEN it ends THEN entry count and last_seen_at unchanged | already_implemented | tests/it/shell_store_readonly.rs |
| C20 | GIVEN a finished run WHEN headings are listed THEN one h1 and sections are h2 | green | tests/it/shell_headings.rs |
| C21 | GIVEN the idle page without JS THEN header, tabs, form, example links and JS notice present | green | tests/it/shell_nojs.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C15) | 1 | sonnet | 26756 | 14 |
| G2 (C1-C14, C16-C21) | 1 | sonnet, two test defects fixed by the orchestrator (ADLC-Test-Change) | 107978 | 270 |
| review fixes (shell_review_p1) | 2 | opus | 86677 | 295 |

### Behavioural verification

Local lens (`netray lens lens.dev.toml`, history on a scratch db):

```
$ curl -s "http://127.0.0.1:18931/inspect/tls?h=example.com:8443" | grep -E "page-inspector|inspector-tabs__tab|query-input__clear|inspector-examples__chip|visually-hidden"
<span class="page-inspector"><span class="page-inspector__sep" aria-hidden="true">·</span> <span class="page-inspector__name">TLS inspector</span></span><span class="page-tagline">TLS, illuminated</span>
<a class="inspector-tabs__tab" href="/inspect/dns?q=example.com">DNS</a>
<a class="inspector-tabs__tab" href="/inspect/tls" aria-current="page">TLS</a>
<a class="inspector-tabs__tab" href="/inspect/http?url=https%3A%2F%2Fexample.com%2F">HTTP</a>
<a class="inspector-tabs__tab" href="/inspect/email?d=example.com">EMAIL</a>
<a class="inspector-tabs__tab" href="/inspect/ip">IP</a>
  <button type="button" class="query-input__clear" aria-label="Clear input">×</button>
<li><a class="inspector-examples__chip" href="/inspect/tls?h=example.com" data-example="example.com">example.com</a></li>
<li><a class="inspector-examples__chip" href="/inspect/tls?h=netray.info" data-example="netray.info">netray.info</a></li>
<div id="inspect-result"><h1 class="visually-hidden">TLS inspector</h1></div>

$ curl -s -X POST -d "h=example.com:443" http://127.0.0.1:18931/inspect/tls/run   (first fragment, abridged)
<div class="inspector-run__head"> ... live · 2026-10-20 09:06 UTC · not recorded ...
<a class="inspector-backlink" href="/example.com">Back to the result of example.com →</a></div>
<h1 class="inspector-run__target">example.com:443</h1>
<section class="inspector-section" id="inspect-part-0"><h2>handshake</h2><p>pending</p></section>
```

The run itself ends in "DNS resolution failed": outbound DNS of the binary is blocked locally.

### Review

First reader: 9 blockers (IP h1 empty for the caller's address, HTTP and TLS h1 not as read,
notices removed the only h1, chips stayed during a run, clear button not synced at mount, HTTP
address and TLS IPv6 targets not carried to the IP tab, card label uppercased, clear button outside
the input), all fixed through `tests/it/shell_review_p1.rs` and
`islands/test/shell-review-p1.test.ts`. Amendments folded into the spec: tabs follow a posted run
(an `inspector-tabs` fragment) and carry only hosts TLS and HTTP take; the clear button is
server-hidden; an incomplete entry shows no letter; a modified chip click stays a link; the grade
read is bounded (500 ms) and logged on failure.
Second reader: 0 blockers. Amendment (a notice replaces the chips) answered in requirement 5.
Deferred: the EMAIL tab answers 404 until Phase 7. Nits (IP h1 as typed, TLS port list as typed)
left as typed input.

## Phase 2 — Band position and live check list

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8 Band directly under the h1 before every part; each line named by catalogue short and line code; caption names the band, count and positions | green | tests/it/shell_band.rs |
| C2 | R9 Live check list: one check row per catalogue check of the band protocol, catalogue order, same function and partial as the stored view, heading counts | green | tests/it/shell_checks.rs |
| C3 | R10 Same rows, same words: live list equals the stored view's rows for TLS and HTTP fixtures | green | tests/it/shell_same_rows.rs |
| C4 | GIVEN a TLS run WHEN the stream ends THEN the band element precedes every part element | green | tests/it/shell_band.rs |
| C5 | GIVEN the TLS band (count and positions from the catalogue) THEN each line has short and code and the caption names them | green | tests/it/shell_band.rs |
| C6 | GIVEN an HTTP run where http.csp warns with a finding THEN its row shows warn, title, finding, fix hint, guide link | green | tests/it/shell_checks.rs |
| C7 | GIVEN a TLS fixture WHEN rendered live and as the stored view THEN the check rows are identical | green | tests/it/shell_same_rows.rs |
| C8 | GIVEN one fail and two warns THEN heading 3 lines need attention with 1 fail · 2 warn; all pass THEN All n lines pass | green | tests/it/shell_checks.rs |
| C9 | GIVEN a DNS query for www.example.com THEN the list holds the DNS checks of example.com | green | tests/it/shell_checks.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1+G2 (one coder, G1 first) | 1 | sonnet | 74885 | 136 |
| review fixes (shell_review_p2) | 1 | orchestrator (four small edits) | 0 | 0 |

### Behavioural verification

Local lens, `curl -s -X POST --data-urlencode "url=https://1.1.1.1/" http://127.0.0.1:18931/inspect/http/run`:
fragment order `inspect-result`, `inspector-tabs`, `inspect-part-0`, `inspect-band`; the start
fragment holds `#inspect-band` before `#inspect-part-0`; the band fragment's caption reads "The HTTP
band of the spectrum, zoomed: the same 12 lines as positions HTTP·32–43 on the result page.", the
Checks heading "No line needs attention" with "12 not measured" (the probes time out locally: the
sandbox blocks most outbound traffic), each row's body the probe's finding ("HTTPS probe: timed out
after 10s").

### Review

Reader: 1 blocker (the live list linked blocks the page does not show; the DNS band linked the
queried name's records although it judges the registrable domain), fixed: `live_rows` keeps only
links to blocks on the page and the DNS job's band carries no evidence links
(`tests/it/shell_review_p2.rs`). Amendments folded into requirements 8 and 9: singular wording for
a one-line band; band line names wrap between words and the band scrolls inside its column.
Deferred: the email band's caption says MAIL (the line-code tag) while the plate says EMAIL;
Phase 7 settles it with the email inspector.

## Phase 3 — URL, sharing, history and autocomplete

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R11 URL follows the query: pushState on submit, back and forward restore and re-run, a prefilled URL runs | green | islands/test/shell-url.test.ts |
| C2 | R12 Share link: the final fragment carries the key, the address bar becomes /inspect/tool?r=key, Copy link copies it or opens the share sheet | green | islands/test/shell-url.test.ts, tests/it/shell_shared.rs |
| C3 | R13 Shared runs: one keyed cache, one hour, 1000 runs, server-rendered ?r= page, 404 notice, DNS reuses the API cache, privacy line | green | tests/it/shell_shared.rs |
| C4 | R14 The caller's own address is never shared | green | tests/it/shell_shared_dns.rs |
| C5 | R15 Query history: 20 distinct per inspector, list on focus, keys, clear, storage failures tolerated | green | islands/test/shell-history.test.ts |
| C6 | R16 Autocomplete hook with the DNS provider from the DNS crate's tables | green | islands/test/shell-history.test.ts, tests/it/shell_suggest.rs |
| C7 | GIVEN the TLS page WHEN example.com:443 is submitted THEN the address bar reads /inspect/tls?h=example.com%3A443 and a state was pushed | green | islands/test/shell-url.test.ts |
| C8 | GIVEN two submitted queries WHEN back is used THEN the input holds the first query and its run starts | green | islands/test/shell-url.test.ts |
| C9 | GIVEN a finished HTTP run THEN the final fragment carries a 12-hex key and ?r=key renders the same run without a job | green | tests/it/shell_shared.rs |
| C10 | GIVEN a shared run page without JS THEN its run title starts SHARED and its blocks are present | green | tests/it/shell_shared.rs |
| C11 | GIVEN a finished run with key K WHEN Copy link is pressed THEN the clipboard holds /inspect/tool?r=K | green | islands/test/shell-url.test.ts |
| C12 | GIVEN ?r= unknown, malformed or of another tool THEN 404 with the idle shell and the expiry notice | green | tests/it/shell_shared.rs |
| C13 | GIVEN a run older than one hour WHEN its key is requested THEN 404 | green | tests/it/shell_shared.rs |
| C14 | GIVEN a finished DNS inspector run with key K THEN GET /api/v1/dns/results/K answers 200 with its mode and query | green | tests/it/shell_shared_dns.rs |
| C15 | GIVEN a key an API DNS job stored THEN /inspect/dns?r=key renders its blocks and the band text | green | tests/it/shell_shared_dns.rs |
| C16 | GIVEN an IP run with empty input THEN no key and no cached run; GIVEN ip=192.0.2.24 THEN a key | green | tests/it/shell_shared_dns.rs |
| C17 | GIVEN a shareable run WHEN it ends THEN the history store is unchanged | green | tests/it/shell_shared.rs |
| C18 | GIVEN 21 distinct submitted queries THEN the history list holds the newest 20, newest first, distinct | green | islands/test/shell-history.test.ts |
| C19 | GIVEN storage that throws WHEN a query is submitted THEN the run starts and no error escapes | green | islands/test/shell-history.test.ts |
| C20 | GIVEN DNS input example.com @cl THEN the provider suggests @cloudflare and Tab completes it | green | islands/test/shell-history.test.ts |
| C21 | GIVEN the TLS page THEN no provider is registered and no suggestion list shows | green | islands/test/shell-history.test.ts, tests/it/shell_suggest.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 server | 2 | opus | 133996 | 656 |
| G2 island | 1 | sonnet | 47657 | 74 |
| review fixes (shell_review_p3) | 1 | orchestrator | 0 | 0 |

### Behavioural verification

Local lens:

```
$ curl -s -X POST --data-urlencode "url=https://1.1.1.1/" http://127.0.0.1:18931/inspect/http/run   -> share fragment data-share-key="a788285f4898"
$ curl -s -w "%{http_code}" "http://127.0.0.1:18931/inspect/http?r=a788285f4898"
200   (no data-stream-autostart; run title parts: shared · 2026-10-20 11:05 UTC · not recorded; h1 https://1.1.1.1/)
$ curl -s -w "%{http_code}" "http://127.0.0.1:18931/inspect/tls?r=a788285f4898"
404
$ curl -s -X POST -d "ip=" http://127.0.0.1:18931/inspect/ip/run | grep -c data-share-key
0
```

### Review

Reader: 2 blockers in the island's popstate (an in-page evidence link re-ran the query or
reloaded; back to the idle entry submitted an empty query), fixed: the shell tracks the path and
query it set and ignores a popstate that keeps them, and loads an entry without a query from the
server. Amendments folded in: suggestions start unselected (Enter submits, Tab completes); an API
DNS result without blocks shows a reason (Rule M); LRU wording in requirement 13 and the SDD.
Tests: `tests/it/shell_review_p3.rs`, `islands/test/shell-review-p3.test.ts`.

## Phase 4 — Help and keyboard

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R17 Help dialog: what the inspector does, syntax with examples, shortcut table, guide link; DNS adds resolvers, flags and types from the server tables; Esc and close button close, focus returns | green | tests/it/shell_help.rs, islands/test/shell-keys.test.ts |
| C2 | R18 Keyboard shortcuts outside text fields; inside the input only Esc and the list keys act | green | islands/test/shell-keys.test.ts |
| C3 | GIVEN any inspector WHEN ? is pressed outside the input THEN the help dialog opens with syntax, shortcut table and guide link | green | tests/it/shell_help.rs, islands/test/shell-keys.test.ts |
| C4 | GIVEN the DNS help THEN it lists the resolvers, flags and record types from the DNS crate's tables | green | tests/it/shell_help.rs |
| C5 | GIVEN the open dialog WHEN Esc is pressed THEN it closes and focus returns to the ? button | green | islands/test/shell-keys.test.ts |
| C6 | GIVEN focus outside the input WHEN / is pressed THEN the input has focus; WHEN Ctrl+L THEN its text is selected | green | islands/test/shell-keys.test.ts |
| C7 | GIVEN a finished run WHEN j is pressed twice THEN the second check row has focus; WHEN Enter THEN its guide link is followed | green | islands/test/shell-keys.test.ts |
| C8 | GIVEN the TLS inspector WHEN l is pressed THEN the HTTP tab's link is followed | green | islands/test/shell-keys.test.ts |
| C9 | GIVEN focus in the input WHEN r is typed THEN it is inserted and nothing re-runs | already_implemented | islands/test/shell-keys.test.ts |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 server | 1 | sonnet | 46692 | 64 |
| G2 island | 1 | sonnet | 32665 | 27 |

### Behavioural verification

`curl -s http://127.0.0.1:18931/inspect/dns` (local lens): the tab row holds
`<button type="button" class="inspector-help__open" aria-haspopup="dialog" aria-controls="inspector-help" aria-label="Help" hidden>`,
the page holds a closed `<dialog class="inspector-help" id="inspector-help" …>` with
`<li><code>@cloudflare</code>` among the resolvers and the guide link `/guide/record-types`.
The keys are verified in `islands/test/shell-keys.test.ts` (happy-dom); a browser pass comes with
Phase 9's screenshots.

### Review

Reader: 0 blockers. Amendments fixed and folded into requirement 18 (tests in
`islands/test/shell-review-p4.test.ts`): `j`/`k` step from the item holding a focused link; `r`
re-runs a shown run of an empty IP input; the help's fallback without `HTMLDialogElement` opens,
closes and returns focus (`hasAttribute("open")`, the Close button no longer submits). Nits fixed:
a closed help is `display: none` where the UA lacks `<dialog>`; Ctrl+Shift+L is left to the
browser; checkboxes and radios no longer swallow shortcuts; Ctrl+L does nothing while the help is
open.

## Phase 5 — Export

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R19 Export data: the final fragment carries Markdown, CSV and JSON templates rendered by the server from the streamed parts and band; a shared run page too; no re-run | green | tests/it/shell_export.rs |
| C2 | R20 Export controls: Copy Markdown, CSV and JSON downloads named netray-tool-slug, disabled until the exports exist, completion announced | green | islands/test/shell-export.test.ts |
| C3 | GIVEN a finished HTTP run THEN the final fragment holds three templates and no further request is made | green | tests/it/shell_export.rs |
| C4 | GIVEN that run WHEN Copy Markdown is pressed THEN the clipboard holds the run title, a check table and a table per block | green | tests/it/shell_export.rs, islands/test/shell-export.test.ts |
| C5 | GIVEN a CSV export THEN header section,item,field,value and every check and block value once | green | tests/it/shell_export.rs |
| C6 | GIVEN a JSON export THEN target, checks and blocks, each block a netray_model Block | green | tests/it/shell_export.rs |
| C7 | GIVEN a run in progress THEN the controls are disabled | green | tests/it/shell_export.rs, islands/test/shell-export.test.ts |
| C8 | GIVEN a TLS run of example.com:443 THEN the JSON file is netray-tls-example.com-443.json | green | tests/it/shell_export.rs, islands/test/shell-export.test.ts |
| C9 | GIVEN a shared run page THEN it holds the same three templates as the live run's final fragment | green | tests/it/shell_export.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 server | 3 | sonnet, one test defect fixed by the orchestrator (ADLC-Test-Change) | 70668 | 383 |
| G2 island | 1 | sonnet | 33829 | 34 |
| review fixes (shell_review_p5) | 1 | orchestrator | 0 | 0 |

### Behavioural verification

Local lens, `curl -s -X POST --data-urlencode "url=https://1.1.1.1/" http://127.0.0.1:18931/inspect/http/run`:
fragments `inspect-result`, `inspector-tabs`, `inspect-part-0`, `inspect-band`, `inspect-share`,
`inspect-export`; the export slot's `data-filename="netray-http-https-1.1.1.1"`; its Markdown
template (unescaped) begins:

```
# https://1.1.1.1/

LIVE · 2026-10-20 12:38 UTC · NOT RECORDED

## Checks

| Code | Check | Status | Finding | Fix |
|---|---|---|---|---|
| HTTP·34 | HTTPS redirect | pass |  |  |
| HTTP·35 | HSTS | fail | No HSTS header found | Send Strict-Transport-Security with max-age=31536000 on all HTTPS responses. |
```

### Review

Reader: 2 blockers, fixed (`tests/it/shell_review_p5.rs`): Markdown left `*`, `\`, `<`, backticks
unescaped (a CSP's wildcards rendered as emphasis); a failed or timed-out part or band vanished
from the exports (Rule M) — now a "Not measured" section, a CSV `error` row and a JSON `errors`
entry. Amendments folded into requirement 19: the `errors` field; a leading `'` for CSV values a
spreadsheet would read as formulas. Deferred, fixed anyway: "Copied Markdown" only after the write
succeeds. Deferred: per-port headings in multi-port TLS Markdown (TLS depth feature). Nit: CSV CRLF
becomes LF through the template's HTML parsing; accepted.

## Phase 6 — Cross-links and IP enrichment

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R21 Address and host links from one function over typed values: addr to the IP inspector, NS/MX/CNAME and the TLS, HTTP, MX hosts to the DNS inspector | green | tests/it/shell_links.rs |
| C2 | R22 IP badges in inspector runs from the IP lookup: org, network type, threat flags in status tones; renderer takes an optional enrichment; stored views none; without modules.ip none | green | tests/it/shell_badges.rs |
| C3 | R23 Related records box at the end of a TLS run linking the DNS inspector with CAA | green | tests/it/shell_links.rs |
| C4 | GIVEN a DNS run whose dns.a block holds 192.0.2.24 THEN the value links to /inspect/ip?ip=192.0.2.24 | green | tests/it/shell_links.rs |
| C5 | GIVEN a TLS run whose tls.addresses holds 2001:db8::24 THEN the link percent-encodes the address | green | tests/it/shell_links.rs |
| C6 | GIVEN an IP lookup stub with org, cloud and Tor for 192.0.2.24 WHEN an HTTP run renders its server address THEN three chips show, Tor in the fail tone | green | tests/it/shell_badges.rs |
| C7 | GIVEN a run with email.mx_addresses WHEN rendered live THEN each address carries its badges | green | tests/it/shell_badges.rs |
| C8 | GIVEN no modules.ip WHEN any inspector renders THEN no badge and the run is complete | green | tests/it/shell_badges.rs |
| C9 | GIVEN the stored view /example.com/tls THEN its addresses link to the IP inspector and carry no badges | green | tests/it/shell_links.rs |
| C10 | GIVEN a TLS run of example.com:443 THEN the related-records box links to /inspect/dns?q=example.com%20CAA | green | tests/it/shell_links.rs |
| C11 | GIVEN the renderer's source WHEN scanned THEN it names no module or block | green | tests/it/shell_links.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1+G2 (one coder) | 3 | opus | 112449 | 646 |
| review fixes (shell_review_p6) | 1 | orchestrator | 0 | 0 |

### Behavioural verification

Local lens (`lens.dev.toml`, no `[modules.ip]`): an HTTP run of `https://1.1.1.1/` renders the
server address as `<a class="evidence-value__link" href="/inspect/ip?ip=1.1.1.1">` and no badge
(no IP module: none expected); a TLS run of `1.1.1.1:443` has no related-records box (an address
target has no host). Badges with an IP module are covered by `tests/it/shell_badges.rs`.

### Review

Reader: 2 blockers, fixed (`tests/it/shell_review_p6.rs`): an NS answer's A/AAAA glue linked to the
DNS inspector (links now follow the row's record type, passed as `data:<TYPE>`); the network chip
read the threat classification (`network_type`) instead of the infrastructure (`infra_type`).
Amendments folded into requirement 21: CNAME rows link in any block; trees stay text (theme D).
Deferred: badges on the API-cache fallback page. Planner finding folded in: the IP inspector's own
run is not enriched (its run is the lookup; requirement 22).
