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
| BLOCKER | phone compact table at 13 px: `.evidence-tabs__panel table` outranks `.evidence-tabs__compact` | `.evidence-tabs__panel .evidence-tabs__compact` takes the size in the phone block; the range review found it still resolved to the 11 px phone label, now token `font.size.evidence` (13 px, 12 px at phone width) |
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

## Phase 3 — Design gate states and canvas

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R14 a board's optional state names its files, rows, summary lines and accepted entries; one title and state twice is refused | green | tests/repo/test_design_manifest.sh, tests/repo/test_design_compare.sh |
| C2 | R15 the long-domain fixture: four addresses, three-item chains, handshakes, renders fresh, validates | green | tests/repo/test_design_fixtures.sh, tests/repo/test_design_states.sh |
| C3 | R16 the private manifest's long-domain states and evidence and run title rows; design-check exits 0 | green | design-check |
| C4 | R17 canvas F and M draw descriptor, cached and All evidence; copies committed in netray-ops; waiting rows listed | green | canvas (netray-ops c7b62df) |
| C5 | S two boards on one title with different states load; the same title and state twice is refused naming the slug | green | tests/repo/test_design_manifest.sh |
| C6 | S a state board's files are `<title slug>-<state slug>-*`, its rows name "<title> [<state>]", an accepted entry matches by that name | green | tests/repo/test_design_compare.sh |
| C7 | S every seeded fixture renders fresh and its domain is an example.com name | green | tests/repo/test_design_fixtures.sh |
| C8 | S `just design-check` exits 0 on the private manifest | green | design-check |

C3 and C8 are measured by `just design-check` (no cargo or shell test can render the private canvas); C4 is the canvas change itself (netray-ops c7b62df, canvas v45).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C1, C5, C6) | 0 | orchestrator (manifest.mjs boardName/boardSlug, four call sites) | 0 | 0 |
| G2 (C2, C7) | 0 | orchestrator (generated fixture) | 0 | 0 |

The fixture test assumed an unbracketed IPv6 qualifier; SDD 3.7 brackets it (`tls.chain@[2001:db8::10]:443`), so the test was corrected under ADLC-Test-Change.

### Design check

`just design-check` on the private manifest (netray-ops 3c45766), canvas export of 2026-10-20 (v43):

```
seeded 7 fixture(s) into …/history.db
0 open deviation(s), 0 stale accepted entries
```

| board | measured elements | open | accepted |
|---|---|---|---|
| D3 · Analyzer — history, light | 108 | 0 | 1 (history rows) |
| D3 · Analyzer — history, dark | 108 | 0 | 1 (history rows) |
| D3 · Analyzer — first visit, light | 101 | 0 | 0 |
| D3 · Analyzer — first visit, dark | 101 | 0 | 0 |
| D3 · History: trace with time lens, light | 117 | 0 | 0 |
| D3 · History: trace with time lens, dark | 117 | 0 | 0 |
| D3 · Mobile result, light | 50 | 0 | 0 |
| D3 · Mobile result, dark | 50 | 0 | 0 |
| D3 · Analyzer — first visit, light [long domain] | 17 | 0 | 0 |
| D3 · Mobile result, light [long domain] | 14 | 0 | 0 |

New rows: "evidence caption" (+ colour), "evidence th" (+ text, colour), "evidence td" (+ colour), "evidence table" on board M, "h1 label box" on every board, "title column height" on the desktop boards; "evidence panel" now matches only a panel without a full evidence block. Against the page before this feature, "evidence panel" is a class-1 row on every board (the TLS tab held every block) and "title column height" deviates on the first-visit boards (the wrapped run title adds a 13 px line to the 69 px column, beyond the 4 px tolerance; reasoned from the measured line height, not re-run against the old build): the gate now catches items 1 and 3. A wrap point (item 4) is not a computed value: the long-domain states render it side by side, and "h1 label box" pins the mechanism.

Visual reading of the side-by-side images: the TLS tab matches board F's chain table (caption, five columns, strong leaf subject and date, feeds line, links); the phone page is 3041 px against board M's 3041 px. At 390 px the first visit's run title wraps after "CACHED" (requirement 12 covers 1280 px only; board M draws no first-visit state).

### Waiting for the operator's canvas re-export

The canvas boards F and M changed in v45 (netray-ops c7b62df); `export/canvas.html` is still v43, so these rows are not yet in the manifest and the reader still sees them as extra on the page:

| board | element to add | board selector | page selector | class, properties |
|---|---|---|---|---|
| F, FDark, FFirst, FFirstDark, FTrace, FTraceDark | grade descriptor | `span:text-is("ok, weaknesses worth fixing")` | `#result .grade__descriptor` | typography: font-size, font-family; colour: color |
| M, MDark | grade descriptor | `div[style*="border-left: 4px"] > span:nth-child(3)` | `#result .grade__descriptor` | typography: font-size, font-family; colour: color |
| F family, M, MDark | all evidence link | `a:text-is("All evidence →")` | `.evidence-tabs__panel[data-shown] .evidence-tabs__all` | typography: font-size |
| F family, M | run title (cached) | text now ends "· cached" | — | no new row: the existing run title rows keep measuring |

### Review

Reader over the phase diff: no blocker.

| class | finding | disposition |
|---|---|---|
| NIT | no test drives the file names and summary lines through check.mjs end to end (only `boardSlug` and `boardName` directly) | listed; the design run above wrote `…-long-domain-{board,page,sbs}.png` and "[long domain]" summary lines |
| NIT | the generated root certificate copied the intermediate's `days_remaining`, an RSA key beside an ECDSA signature and a placeholder fingerprint | repaired: days to 2040-01-01, ECDSA P-384, a SHA-256 of the subject as fingerprint |
| DEFERRED | verdicts inherited from `new.example.com.json` contradict their blocks (passing header checks without the headers in `http.headers`, DMARC warn with p=reject); the compact HTTP table now shows "not sent" beside a passing line | the design fixtures' check statuses predate the compact tables; a fixture-consistency pass belongs to a later fixture feature |
