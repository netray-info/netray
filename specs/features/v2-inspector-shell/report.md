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
| C5 | GIVEN the TLS band (21 lines, TLS·11–31) THEN each line has short and code and the caption names them | green | tests/it/shell_band.rs |
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
