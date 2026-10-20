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
