# Report: v2-result-polish

## Phase 1 — Compact evidence tabs

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 a stored entry's panel holds reason, caption, one table, swipe note, links; no evidence-block; no caption or table without source blocks | green | crates/web/tests/it/polish_compact_tls.rs |
| C2 | R2 the feeds text names only the lines whose checks reference the table's source blocks | green | crates/web/tests/it/polish_compact_feeds_live.rs |
| C3 | R3 a live or unrecorded run holds the table and a closed details "All evidence" with every block, no link | green | crates/web/tests/it/polish_compact_feeds_live.rs |
| C4 | R4 a cell over 64 characters shows 63 and an ellipsis, the full value in title | green | crates/web/tests/it/polish_compact_http.rs |
| C5 | R5 TLS chain table of the band port, caption, columns, cell rules, addresses fallback | green | crates/web/tests/it/polish_compact_tls.rs |
| C6 | R6 DNS key records table: A, AAAA, NS, CAA, DS; "no records" row; answered-types caption | green | crates/web/tests/it/polish_compact_dns.rs |
| C7 | R7 HTTP response caption and the six security headers, "not sent" | green | crates/web/tests/it/polish_compact_http.rs |
| C8 | R8 Email mail records rows in order, MX joined, one DKIM row per key | green | crates/web/tests/it/polish_compact_email_ip.rs |
| C9 | R9 IP addresses table, Listed flags or "none" | green | crates/web/tests/it/polish_compact_email_ip.rs |
| C10 | R10 compact caption and table styles on desktop and phone | green | crates/web/tests/it/polish_compact_styles.rs |
| C11 | S two identical four-item chains: consistent caption, header, four rows, first row values and strong cells, no evidence-block | green | crates/web/tests/it/polish_compact_tls.rs |
| C12 | S differing fingerprints: "chains differ · showing 192.0.2.10" | green | crates/web/tests/it/polish_compact_tls.rs |
| C13 | S self_signed root reads "self-signed"; "RSA 2048" stays | green | crates/web/tests/it/polish_compact_tls.rs |
| C14 | S no chain block: "Addresses · n tested", columns Address, Port, Outcome, Message | green | crates/web/tests/it/polish_compact_tls.rs |
| C15 | S dns.a two records and empty dns.caa: A rows, CAA "no records" row, caption count | green | crates/web/tests/it/polish_compact_dns.rs |
| C16 | S no HSTS header: "not sent"; caption starts "Response · 200 · h2" | green | crates/web/tests/it/polish_compact_http.rs |
| C17 | S email MX, SPF, DMARC, one DKIM: rows in order, "Mail records · 1 MX host" | green | crates/web/tests/it/polish_compact_email_ip.rs |
| C18 | S IP tor true reads "tor"; all false reads "none" | green | crates/web/tests/it/polish_compact_email_ip.rs |
| C19 | S TLS feeds name tls.chain_trusted, not tls.tls_version | green | crates/web/tests/it/polish_compact_feeds_live.rs |
| C20 | S 200-character CSP: 63 characters and an ellipsis, full value in title | green | crates/web/tests/it/polish_compact_http.rs |
| C21 | S a live section fragment holds the table and a closed details "All evidence", no link | green | crates/web/tests/it/polish_compact_feeds_live.rs |
| C22 | S stylesheets resolve caption, header and body cells to the board values | green | crates/web/tests/it/polish_compact_styles.rs |

C1's panel-without-sources test (`c1_a_panel_without_compact_sources_keeps_only_the_links_line`) passed before the change: kept, it protects that behaviour.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 3 | sonnet (13 of 16; the last 3 were test bugs fixed by the orchestrator under ADLC-Test-Change) | 88556 | 218 |

### Behavioural verification

`just build && just design-check` on the seeded design stack:

```
seeded 6 fixture(s) into …/history.db
0 open deviation(s), 0 stale accepted entries
```

Page heights: board M side-by-side 3041 px (the phone page now fits under board M's 3041 px; it was 6765 px), board F page 2006 px. The TLS tab reads "Certificate chain · port 443 · 2 addresses, consistent", a four-row chain table, the chain lines' feeds, "All evidence →" and "Open in the TLS inspector →".

### Review

Reader over the phase diff: 5 blockers, 2 amendments, 1 nit; all repaired in this phase (second pass green).

| class | finding | repair |
|---|---|---|
| BLOCKER | phone compact table at 13 px: `.evidence-tabs__panel table` outranks `.evidence-tabs__compact` | `.evidence-tabs__panel .evidence-tabs__compact` sets 12 px in the phone block |
| BLOCKER | phone `display: block; overflow-x: auto` of evidence-block tables also hit the compact table: two nested scroll areas | the same rule restores `display: table; overflow-x: visible`; `.evidence-tabs__scroll` is the one scroller |
| BLOCKER | `main td` made body cells muted with a bottom rule | `.evidence-tabs__compact td` ink, no bottom rule; th no bottom rule |
| BLOCKER | DKIM key without type rendered "mail: revoked," | empty key type and bits count as unknown |
| BLOCKER | no `email.mx` block still read "0 MX hosts" | caption "Mail records" without a count (spec requirement 8 amended) |
| AMENDMENT | repeated `record` fields (two SPF records) showed only the first | one row per record (requirement 8 amended) |
| AMENDMENT | Ed25519 and Ed448 kept their fixed size | the size is dropped after Ed* too (requirement 5 amended) |
| NIT | swipe note after the disclosure | swipe note right after the table; the disclosure after the links line |

Edge-case tests for the repairs: `crates/web/tests/it/polish_compact_review.rs`.

## Phase 2 — Hero: descriptor, run title, long domains

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R11 the descriptor stays and does not set the grade block's width; it wraps | green | crates/web/tests/it/polish_hero.rs |
| C2 | R12 at 1280 px every fixture's run title is one line tall | test-unwritable | design-check (run title height rows, Phase 3) |
| C3 | R13 the h1 has a break point after every dot and no other; overflow-wrap anywhere as last resort on both widths | green | crates/web/tests/it/polish_hero.rs |
| C4 | S the descriptor stylesheet: width 0 and min-width 100 % | green | crates/web/tests/it/polish_hero.rs |
| C5 | S the long domain renders with wbr after each dot | green | crates/web/tests/it/polish_hero.rs |
| C6 | S `#result-header h1` carries overflow-wrap anywhere on desktop and phone | green | crates/web/tests/it/polish_hero.rs |
| C7 | S the first-visit run title is one line at 1280 px in the design stack | test-unwritable | design-check (Phase 3) |

C2 and C7 are layout facts a cargo test cannot see: the design check measures them (Phase 3 adds the run title height rows).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C1, C3–C6) | 0 | orchestrator (three CSS and template lines) | 0 | 0 |

### Review

| class | finding | repair |
|---|---|---|
| BLOCKER | a hyphen is a browser break point: "checkout.my-" / "company…" at 390 px, punycode "xn--mnchen-" too | each label with its dot is an inline-block box (`.result-hero__label`); measured in Chromium: "checkout." / "my-company." / "example.com"; a 50-character label still wraps inside its box, no page scroll (R13 amended) |
| DEFERRED | the `/r/<id>` entry page and the history page h1 carry no break points | outside R13 (the result hero); a later page feature |

### Behavioural verification

`just design-check`: 0 open deviations. First visit at 1280 px: the run title "RUN 1 · … · 54 CHECKS · PROFILE 3 · CACHED · FIRST RUN" is one line, the descriptor wraps under the score in two lines. Board M at 390 px: "shop.example.com" on one line beside the grade.
