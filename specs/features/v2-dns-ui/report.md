# Report: v2-dns-ui

## Phase 1 — Summary blocks, the part view and the Results tab

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 query, compare and authcompare parts gain dns.answers and dns.servers trees from the summary and done; absent facts omitted; API-cache results carry them | green | crates/dns/tests/it/job_page_blocks.rs |
| C2 | R2 a part-level view claims dns.answers, dns.servers, dns.warnings and fully shown job blocks, renders one fragment with an ev anchor per claimed block | green | crates/web/tests/it/dns_view_part.rs |
| C3 | R3 run summary line with counts, transport and duration; warnings and degraded providers as notices; unfinished notice | green | crates/web/tests/it/dns_view_summary.rs |
| C4 | R4 one card per type with its agreement badge and the compare and authcompare agree words | green | crates/web/tests/it/dns_view_results.rs |
| C5 | R5 answers grouped by identical data with server count, server names, TTL range and a Rust-computed bar | green | crates/web/tests/it/dns_view_results.rs |
| C6 | R6 no records, NXDOMAIN and not measured with the reason beside answered groups | green | crates/web/tests/it/dns_view_negative.rs |
| C7 | R7 record data in mono with the differing human form under it | green | crates/web/tests/it/dns_view_readable.rs |
| C8 | S example.com A part holds dns.answers and dns.servers before dns.a, type A agree | green | crates/dns/tests/it/job_page_blocks.rs |
| C9 | S example.com A Results tab shows an agree A card, 192.0.2.10 and m / m servers; generic dns.a gone | green | crates/web/tests/it/dns_view_part.rs |
| C10 | S diverge.example.com A reads diverge with one group per distinct answer | green | crates/web/tests/it/dns_view_results.rs |
| C11 | S partial.example.net AAAA reads not measured with the reason; summary holds not measured | green | crates/web/tests/it/dns_view_summary.rs |
| C12 | S one server SERVFAIL beside an answer reads not measured: SERVFAIL | green | crates/web/tests/it/dns_view_negative.rs |
| C13 | S all servers NODATA reads no records | green | crates/web/tests/it/dns_view_negative.rs |
| C14 | S an SPF TXT record shows its human form | green | crates/dns/tests/it/job_page_blocks_human.rs, crates/web/tests/it/dns_view_readable.rs |
| C15 | S a done warning shows as a notice line and the Warnings table is gone | green | crates/web/tests/it/dns_view_summary.rs |
| C16 | S bars for 10 and 40 ms are a quarter and full | green | crates/web/tests/it/dns_view_results.rs |
| C17 | S compare agree reads all transports agree; authcompare reads auth and recursive agree | green | crates/web/tests/it/dns_view_results.rs |
| C18 | S a records block with a record no answer holds renders generically | green | crates/web/tests/it/dns_view_part.rs |
| C19 | S nothing lost: every record data of the claimed blocks appears in the fragment | green | crates/web/tests/it/dns_view_part.rs |
| C20 | S a cached API result key renders the Results tab | green | crates/web/tests/it/dns_view_part.rs |

C10's test asked for two servers from `@google @cloudflare`; the parser expands each provider to its
two addresses, so the card lists four servers in two groups. The count was corrected to 4
(`ADLC-Test-Change`). `inspect_dns_jobs.rs` c8 now asserts the claimed compare and authcompare
tables by their anchor inside the DNS view.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 78150 | 164 |
| G2 | 2 | opus | 137380 | 760 |

G2 ended with one red test whose expectation was wrong about the stub (above); the review fixes
below were made by the orchestrator.

### Review

| class | finding | disposition |
|---|---|---|
| BLOCKER | claimed records lost the shell's links and address badges (ctx.decorate unused) | fixed: record data decorated with key `data:<type>`; test `dns_view_review.rs` `record_data_keeps_the_shells_links` |
| BLOCKER | a referral read "not measured: NOERROR"; a not-measured block or failed table row was claimed without its reason shown | fixed: error before response code; claims check the reason; test `a_referral_reads_its_error_and_its_block_is_claimed_only_with_that_reason` |
| AMENDMENT | a group TTL range could hide a record's own TTL held by the merged records block | requirement 5 amended, repaired: each record's TTL range in its details; test `each_record_shows_its_own_ttl` |
| AMENDMENT | dns.answers root values total_queries, dnssec, single were never shown | requirement 3 amended, repaired: single in the summary line, a meta line for queries sent and DNSSEC; unfinished counts stay unshown (the cards hold every answer) |
| AMENDMENT | the API-cache replay lacked dns.errors and dns.warnings | requirement 1 amended, repaired in `crates/web/src/api/jobs.rs` |
| NIT | protected test edits need ADLC-Test-Change | the commit carries the trailers |

### Behavioural verification

Local server over the offline DNS stub (`cargo run -p netray-web --example dns_stub 18533`, new in
this phase for the visual reading): `curl -s -X POST --data-urlencode 'q=diverge.example.com A @google @cloudflare' http://127.0.0.1:18533/inspect/dns/run`:

```
dns-view__summary">1 type · 4 lookups · 4 servers · udp · 0 agree · 1 diverge · 0 ms
dns-type__badge" data-tone="warn">diverge
dns-group__count">3 / 4 servers
dns-group__count">1 / 4 servers
```

## Phase 2 — Servers tab, per-server view and tab badges

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8 one server card per server in summary order with transport, source, measured count, median and per type outcome and records; diverge marker per diverging type | green | crates/web/tests/it/dns_view_servers.rs |
| C2 | R9 a server filter sets data-server on the result region and hides other servers' answers in both tabs; All servers resets; without JavaScript no filter | green | crates/web/islands/test/dnsview.test.ts, crates/web/tests/it/dns_view_servers.rs |
| C3 | R10 Results tab carries d diverge and u not measured or the pass mark; Servers tab carries servers with an unmeasured lookup or the pass mark; counts from the server | green | crates/web/tests/it/dns_view_badges.rs |
| C4 | S several servers give one card each in summary order with transport, measured and median | green | crates/web/tests/it/dns_view_servers.rs |
| C5 | S a server with only errors shows no median and its lookups as not measured | green | crates/web/tests/it/dns_view_servers.rs |
| C6 | S a diverging A type is marked divergent in every server card | green | crates/web/tests/it/dns_view_servers.rs |
| C7 | S the filter button sets data-server; All servers removes it | green | crates/web/islands/test/dnsview.test.ts |
| C8 | S one diverging and one unmeasured type give Results badges 1 diverge and 1 not measured | green | crates/web/tests/it/dns_view_badges.rs |
| C9 | S every type agreeing gives the Results pass mark | green | crates/web/tests/it/dns_view_badges.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 61684 | 97 |

The plan was written by the orchestrator: one group, its files named by the Phase 1 view.

### Review

| class | finding | disposition |
|---|---|---|
| BLOCKER | the filter left other servers' answer groups visible in the Results tab | fixed: a group with no shown server is off too; test `islands/test/dnsview-review.test.ts` |
| BLOCKER | in authcompare a recursive and an authoritative server printing alike shared one card answer and one filter key | fixed: such servers get a source-qualified filter key (`data-filter-key`), cards match source and transport; test `dns_view_review2.rs` |
| AMENDMENT | an unfinished job showed pass marks on both tabs | requirement 10 amended, repaired: no pass mark when unfinished; test `an_unfinished_job_has_no_pass_mark` |
| NIT | a nameless server card read the mhost string | fixed: it reads its address |
| NIT | Servers tab record data has no links | kept: the Results tab carries them (Decision log) |
| NIT | two test checks cannot fail (summary order found in the Results tab first; a must-not-contain needle) | noted |

### Behavioural verification

`curl -s -X POST --data-urlencode 'q=partial.example.net A AAAA @google' http://127.0.0.1:18533/inspect/dns/run` against the stub server:

```
dns-view__tab-badge" data-tone="muted">1 not measured
dns-view__tab-badge" data-tone="warn">2 not measured
dns-server__measured">1 / 2 measured
dns-server__outcome">not measured: SERVFAIL
```

## Phase 3 — Toggles, explain and record details

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R11 hidden toolbar with hide empty, compact, only divergent, sort, expand and collapse; data attributes on the region; stylesheet hides; reorder by written ranks; persisted in netray.dns.view | green | crates/web/tests/it/dns_view_details.rs, crates/web/islands/test/dnsview-toolbar.test.ts |
| C2 | R12 explain toggle shows the server-rendered explain sentence, hidden until on | green | crates/web/tests/it/dns_view_details.rs |
| C3 | R13 record details: TTL in human units with seconds, server as mhost names it, transport, response time; SOA serial age on the row | green | crates/web/tests/it/dns_view_details.rs |
| C4 | R18 without JavaScript tabs in order, details open, toolbar hidden, explain absent from view | green | crates/web/tests/it/dns_view_details.rs |
| C5 | R19 bundle under 30 KB gzipped; islands write no style and compute no count | green | crates/web/islands/test/budget.test.ts, crates/web/islands/test/dnsview-toolbar.test.ts |
| C6 | S hide empty sets data-hide-empty and writes no style | green | crates/web/islands/test/dnsview-toolbar.test.ts |
| C7 | S only divergent sets data-only-divergent and the stylesheet hides non-diverging cards | green | crates/web/islands/test/dnsview-toolbar.test.ts, crates/web/tests/it/dns_view_details.rs |
| C8 | S slowest first orders cards by data-rank | green | crates/web/islands/test/dnsview-toolbar.test.ts |
| C9 | S a stored choice starts the toggles in that state; throwing storage keeps defaults | green | crates/web/islands/test/dnsview-toolbar.test.ts |
| C10 | S an MX explain sentence is in the markup inside an element hidden until explain | green | crates/web/tests/it/dns_view_details.rs |
| C11 | S TTL 3900 reads 1 h 5 min (3900 s) with server, transport and response time | green | crates/web/tests/it/dns_view_details.rs |
| C12 | S SOA serial 2026101501 aged 4 reads serial 2026101501 · 4 days old; no age no text | green | crates/web/tests/it/dns_view_details.rs |
| C13 | S expand all opens every record's details | green | crates/web/islands/test/dnsview-toolbar.test.ts |
| C14 | S no JavaScript: toolbar hidden, details still open | green | crates/web/tests/it/dns_view_details.rs |
| C15 | S the bundle budget test passes | already_implemented | crates/web/islands/test/budget.test.ts |

C15 passed on the baseline (the budget test exists since v2-design-system); the bundle is 26.6 kB
minified after this phase. C10's markup half passed on the baseline (Phase 1 rendered the explain
sentence); its stylesheet half and the review's placement change are new.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 60457 | 96 |

### Review

| class | finding | disposition |
|---|---|---|
| BLOCKER | Compact hid a not-measured group placed after the answered one | fixed: Compact keeps the lead (first answered) group and every unmeasured group; test `dns_view_review3.rs` |
| BLOCKER | Compact plus a server filter or Hide empty could empty a card | fixed: Compact steps aside under a server filter; the lead is the first answered group |
| BLOCKER | Compact did not hide record details | fixed: `.dns-view[data-compact] .dns-record__details` |
| BLOCKER | the explain sentence sat inside the closed details, so Explain showed nothing | fixed: the sentence follows the details; c10's needle moved (`ADLC-Test-Change`) |
| AMENDMENT | Only divergent hid not-measured cards | requirement 11 amended, repaired |
| AMENDMENT | the details' server rows ignored the server filter | requirement 13 amended, repaired: `data-server` on each row, island selector extended |
| AMENDMENT | a serial dated after today read a negative age | requirement 13 amended, repaired: "dated in the future" |

### Behavioural verification

Screenshots from the stub server (`node shoot.cjs` in the scratchpad, Playwright over
`cargo run -p netray-web --example dns_stub`): every state at 1280 px and 390 px, light and dark,
has `document.documentElement.scrollWidth` equal to the viewport (1280, 390). The visual reading is
Phase 6.
