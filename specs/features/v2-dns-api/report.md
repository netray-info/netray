# Report: v2-dns-api

## Phase 1 — Per-server comparison and run summary

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: each lookup maps to a DnsServerAnswer with server, address, name, transport, source, outcome (answered, no_records, nxdomain, unmeasured), response_code, error, response_time_ms, records in presentation form and canonical order | green | crates/dns/tests/job_summary.rs |
| C2 | R2: a SOA record carries soa_serial_age_days for a YYYYMMDDnn serial with a plausible date, none otherwise | green | crates/dns/tests/job_summary.rs |
| C3 | R3: per record type a DnsTypeComparison with agree, diverge, single or unmeasured over measured answers, TTL ignored, sorted | green | crates/dns/tests/job_summary.rs |
| C4 | R4: per server a DnsServerSummary with lookups, measured, unmeasured and the median response time | green | crates/dns/tests/job_summary.rs |
| C5 | R5: collected query, compare and authcompare results carry summary once done arrived; v1 SSE emits one summary event before done; prism emits none | green | crates/web/tests/api_dns_summary.rs |
| C6 | R6: v1 trace keeps server_name, answer_records, referral_ns, referral_groups; v1 dnssec keeps dnskey_records, ds_records, rrsig_records, documented | already_implemented | crates/web/tests/api_dns_summary.rs |
| C7 | GIVEN the stub and POST /dns/query example.com A WHEN finished THEN one A comparison agree, one answered answer per server with 192.0.2.10, transport and response time | green | crates/web/tests/api_dns_summary.rs |
| C8 | GIVEN a stub name whose servers return different A records WHEN queried THEN diverge, each answer keeps its records | green | crates/web/tests/api_dns_summary.rs |
| C9 | GIVEN dnssec-failed.org A WHEN queried THEN every answer unmeasured with SERVFAIL and the comparison unmeasured | green | crates/web/tests/api_dns_summary.rs |
| C10 | GIVEN partial.example.net A and AAAA THEN A agree, AAAA unmeasured, summary.agree 1 and summary.unmeasured 1 | green | crates/web/tests/api_dns_summary.rs |
| C11 | GIVEN a type every server answers NODATA THEN each answer no_records and the comparison agree | green | crates/web/tests/api_dns_summary.rs |
| C12 | GIVEN one measured answer and one SERVFAIL THEN single, not diverge | green | crates/dns/tests/job_summary.rs |
| C13 | GIVEN same records and different TTLs THEN agree | green | crates/dns/tests/job_summary.rs |
| C14 | GIVEN response times 10, 30, 20 THEN median 20; 10 and 30 THEN 20; only errors THEN no median | green | crates/dns/tests/job_summary.rs |
| C15 | GIVEN serial 2026101501 and today 2026-10-19 THEN age 4; serial 12345 or 2026139901 THEN no age | green | crates/dns/tests/job_summary.rs |
| C16 | GIVEN a stub SOA answer WHEN queried THEN the SOA record carries presentation data and soa_serial_age_days | green | crates/web/tests/api_dns_summary.rs |
| C17 | GIVEN POST /dns/compare on the stub THEN each type's comparison holds answers of all four transports, agree | green | crates/web/tests/api_dns_summary.rs |
| C18 | GIVEN POST /dns/authcompare on the stub THEN recursive and authoritative answers with source set, agree | green | crates/web/tests/api_dns_summary.rs |
| C19 | GIVEN POST /dns/query with event-stream THEN one summary event precedes done, equals the JSON summary, valid against DnsJobSummary | green | crates/web/tests/api_dns_summary.rs |
| C20 | GIVEN prism V1 /api/query on the stub THEN no summary event, frames equal v1 frames without summary and cache_key | green | crates/web/tests/api_dns.rs |
| C21 | GIVEN POST /dns/trace on the stub THEN final hop server results carry answer_records 192.0.2.10 and server_name; POST /dns/dnssec THEN dnskey_records, ds_records, rrsig_records per level | already_implemented | crates/web/tests/api_dns_summary.rs |

C6 and C21 passed before any production change: the v1 trace and dnssec results already carried
the detail fields; their tests are kept. C20 also held before (prism never had a `summary`) and now
guards the V1 wire. C11's NODATA case is covered both pure (`job_summary.rs`) and over the router.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 (+1 outside the count for fmt and clippy) | sonnet | 108266 | 312 |

The coder stopped red on `openapi_responses`, whose list of allowed DNS SSE events had to admit
`summary`; that test was changed under `ADLC-Test-Change`, then the group was green.

### Review

- BLOCKER (fixed): an authoritative referral (NOERROR, no answer, not authoritative, NS in
  authority) counted as a measured `no_records`; it now sets `failure` "referral, no answer" and is
  `unmeasured` (Rule M), in the summary and the inspector's authcompare rows alike.
- BLOCKER (fixed): authoritative `response_code` held hickory's description ("Query Refused");
  a new `failure_code` carries the mnemonic (`REFUSED`), as recursive answers do. Test:
  `authoritative_refusal_carries_the_mnemonic`.
- BLOCKER (fixed): recursive and authoritative record text come from different printers, which
  differ on DNSSEC types (RRSIG algorithm name against number, NSEC3PARAM Debug); across sources,
  types outside the aligned list compare by record count. Test:
  `cross_source_dnssec_types_compare_by_count`.
- AMENDMENT (repaired in phase): servers whose query never became a lookup stay in `errors` and
  are absent from `summary`; spec decision log.
- AMENDMENT (repaired in phase): the `summary` frame is now a `Summary` variant of each batch
  job's event enum, so the documented `text/event-stream` schema admits it.

### Behavioural verification

skipped: the binary's outbound DNS is blocked locally (Little Snitch); the v1 routes are driven
end to end over the router and the offline stub in `api_dns_summary.rs`. The live probe is Phase 6's.

## Phase 2 — Query language on GET

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R7: GET /dns/query?q= parses q with the query language and runs the query job as the POST route answers, JSON or SSE, charging [api.dns]; +dnssec adds DNSKEY and DS; warnings in done.warnings; missing or empty q, a parse error 400 INVALID_INPUT; policy refusals as POST | green | crates/web/tests/api_dns_get.rs |
| C2 | R8: GET /dns/compare, authcompare, trace, dnssec with q run that job; trace over the first type, A when none | green | crates/web/tests/api_dns_get.rs |
| C3 | R9: in GET /dns/query, +trace, +compare, +auth or +check answers 307 to /api/v1/dns/trace, compare, authcompare or check with the same q, nothing charged | green | crates/web/tests/api_dns_get.rs |
| C4 | GIVEN q=example.com MX @google THEN 200, batches MX only, every lookup a Google server, valid DnsQueryResult | green | crates/web/tests/api_dns_get.rs |
| C5 | GIVEN q=example.com THEN batches A, AAAA, CNAME, MX | green | crates/web/tests/api_dns_get.rs |
| C6 | GIVEN q=example.com A +dnssec THEN batches A, DNSKEY, DS and done.dnssec true | green | crates/web/tests/api_dns_get.rs |
| C7 | GIVEN q=example.com A bogus THEN 200 and done.warnings names the unrecognized token | green | crates/web/tests/api_dns_get.rs |
| C8 | GIVEN no q or q empty THEN 400 INVALID_INPUT | green | crates/web/tests/api_dns_get.rs |
| C9 | GIVEN q=example.com ANY THEN 400 INVALID_INPUT naming the blocked type | green | crates/web/tests/api_dns_get.rs |
| C10 | GIVEN q=example.com A @192.168.1.1 THEN the POST route's refusal for that server | green | crates/web/tests/api_dns_get.rs |
| C11 | GIVEN Accept text/event-stream on GET /dns/query THEN events batch, summary, done | green | crates/web/tests/api_dns_get.rs |
| C12 | GIVEN GET /dns/compare?q=example.com A THEN DnsCompareResult with four transports; /dns/authcompare THEN DnsAuthCompareResult | green | crates/web/tests/api_dns_get.rs |
| C13 | GIVEN GET /dns/trace?q=example.com MX THEN DnsTraceResult; /dns/dnssec?q=example.com THEN DnsDnssecResult | green | crates/web/tests/api_dns_get.rs |
| C14 | GIVEN GET /dns/query?q=example.com +trace THEN 307 to /api/v1/dns/trace?q=example.com%20%2Btrace; +compare, +auth, +check to compare, authcompare, check | green | crates/web/tests/api_dns_get.rs |
| C15 | GIVEN a caller over the [api.dns] budget WHEN GET /dns/query?q=example.com THEN 429 with Retry-After | green | crates/web/tests/api_dns_get.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 3 | sonnet (+2 test fixes by the implementer) | 65470 | 156 |

The coder ended red on two test-side gaps, fixed under `ADLC-Test-Change`: the five-job table of
`api_dns_get.rs` spent more than the default `[api.dns]` burst from one caller (the table now runs
with raised bursts), and `openapi_responses.rs` had no cases for the new documented GET operations.
The coder also refuses a first token starting with `@` or `+` (the parser takes it as a domain).

### Review

- DEFERRED: `+check` redirects to `/dns/check`, which Phase 5 adds.
- DEFERRED (pre-existing, also POST and V1): `+dnssec` adds DNSKEY and DS after the cost and the
  `max_record_types` check, so a compare is charged for fewer lookups than it runs and 10 types
  plus `+dnssec` run 12.
- NIT: the trace's "A when none" fallback is unreachable (the parser fills default types); an
  address `q` traces PTR.

### Behavioural verification

skipped: outbound DNS blocked locally; the GET forms are driven over the router and the offline
stub. Live probe in Phase 6.

## Phase 3 — Parse, servers and record types

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R10: POST /dns/parse {input, cursor_pos} answers tokens, completions, query, error without running anything; @system only when allowed; behind [api]; malformed body 400 | green | crates/web/tests/api_dns_reference.rs |
| C2 | R11: GET /dns/servers answers providers with configs and the group aliases public and all, behind [api] | green | crates/web/tests/api_dns_reference.rs |
| C3 | R12: GET /dns/record-types answers every type except ANY, AXFR, IXFR, OPT, ZERO, NULL, behind [api] | green | crates/web/tests/api_dns_reference.rs |
| C4 | GIVEN input example.com MX @cl THEN tokens domain, record_type, server_partial with byte ranges, completions include @cloudflare, query.servers empty with a warning naming @cl | green | crates/web/tests/api_dns_reference.rs |
| C5 | GIVEN input example.com A @google +tcp THEN query domain example.com, types A, servers google, transport tcp, error null | green | crates/web/tests/api_dns_reference.rs |
| C6 | GIVEN input example.com ANY THEN query null and error names the blocked type | green | crates/web/tests/api_dns_reference.rs |
| C7 | GIVEN empty input and cursor 0 THEN tokens empty and error the empty-query message | green | crates/web/tests/api_dns_reference.rs |
| C8 | GIVEN a body that is not JSON THEN 400 | green | crates/web/tests/api_dns_reference.rs |
| C9 | GIVEN the default DNS config and input example.com @ THEN completions do not offer @system | green | crates/web/tests/api_dns_reference.rs |
| C10 | GIVEN GET /dns/servers THEN providers include cloudflare, google, quad9, mullvad with udp, tcp, tls, https configs, groups public and all list their providers | green | crates/web/tests/api_dns_reference.rs |
| C11 | GIVEN GET /dns/record-types THEN A, MX, DNSKEY, TLSA, SVCB present, none of ANY, AXFR, IXFR, OPT, ZERO, NULL | green | crates/web/tests/api_dns_reference.rs |
| C12 | GIVEN each of the three routes THEN the body validates against its documented schema | green | crates/web/tests/api_dns_reference.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 3 | sonnet (+1 test fix by the implementer) | 86264 | 200 |

The coder ended red on `openapi_responses.rs`, which needs a 2xx and a non-2xx case per documented
operation; the cases for the three new operations were added under `ADLC-Test-Change`. The module's
`allow_system_resolvers` defaults to true (production sets false): the offline stub now builds its
jobs with false, the production default is unchanged.

### Review

- AMENDMENT (repaired in phase): the C9 scenario is reworded to name the setting, and the `true`
  case on the v1 route gets its own test (committed with Phase 4's tests).
- NIT: `/dns/parse` reports `query` for an input whose first token is `@…` or `+…`, which the GET
  job routes refuse; the parser does not validate domains, so `query` never meant "will run".
- DEFERRED (pre-existing): the V1 tokenizer splits on ASCII whitespace, the parser on Unicode
  whitespace; an input with U+00A0 between tokens reads differently in `tokens` and `query`.

### Behavioural verification

skipped: covered over the router; the live probe of `servers`, `record-types` and `parse` against
a running binary is Phase 6's.

## Phase 4 — Result cache and replay

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R13: every v1 DNS job route stores its collected result at done under a fresh random 12-lowercase-hex key returned in done.cache_key; 1000 results, one hour; no done, no key; DnsDnssecDone gains an optional cache_key absent on prism's wire | green | crates/web/tests/api_dns_results.rs |
| C2 | R14: GET /dns/results/{key} answers key, mode, query, result; malformed key 400 INVALID_INPUT, unknown 404 NOT_FOUND; behind [api] | green | crates/web/tests/api_dns_results.rs |
| C3 | GIVEN POST /dns/query as JSON THEN done.cache_key is 12 lowercase hex and the replay answers mode query, query example.com and a result equal to the original body | green | crates/web/tests/api_dns_results.rs |
| C4 | GIVEN GET /dns/query?q=example.com MX streamed THEN the done event's cache_key replays with query example.com MX and the summary included | green | crates/web/tests/api_dns_results.rs |
| C5 | GIVEN compare, authcompare, trace and dnssec THEN each done carries a key that replays with its mode | green | crates/web/tests/api_dns_results.rs |
| C6 | GIVEN two runs THEN their keys differ | green | crates/web/tests/api_dns_results.rs |
| C7 | GIVEN /dns/results/XYZ or a 13-character key THEN 400 INVALID_INPUT; an unknown 12-hex key THEN 404 NOT_FOUND | green | crates/web/tests/api_dns_results.rs |
| C8 | GIVEN prism's V1 dnssec route streamed THEN its done has no cache_key | already_implemented | crates/web/tests/api_dns_results.rs |
| C9 | GIVEN a replay THEN it validates against DnsCachedResult's documented schema | green | crates/web/tests/api_dns_results.rs |

C8 held before the change (prism's dnssec route never set a key) and guards the V1 wire.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 3 | sonnet (+2 test fixes by the implementer) | 83730 | 296 |

Test-side gaps fixed under `ADLC-Test-Change`: `api_dns.rs` compared v1 frames, which now carry
`cache_key`, with prism's while stripping the key from prism's only; `openapi_responses.rs` needed
cases for `GET /dns/results/{key}` (its 2xx case runs a query first for a live key).

### Review

- BLOCKER (fixed): moka's default TinyLFU admission dropped an unread new result once the cache
  was full, so a freshly handed-out key could answer 404; the cache now evicts LRU. Test:
  `crates/dns/tests/result_cache.rs` (fails without the fix).
- AMENDMENT (repaired): requirement 14 says "12 lowercase hex characters", as the code and the
  document do.
- NIT (fixed): the JSON path summarised twice (two clocks); a collected `summary` event now wins
  over recomputing at `done`, so the answer and the stored replay hold the same summary.

### Behavioural verification

skipped: covered over the router; live probe in Phase 6.

## Phase 5 — DNS health check

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R15: POST /dns/check {domain} and GET /dns/check?q= run the DNS module alone, live, not stored, answering protocol, outcome, reason, checks, blocks, cache_key as JSON whatever the Accept; IP, empty or invalid name 400; flat 16 on [api.dns]; 30 s deadline; no module 503 | open | |
| C2 | GIVEN the golden DNS module and POST /dns/check example.com THEN 200, protocol dns, outcome measured, the ten dns checks with status and findings, the evidence blocks, a cache_key replaying with mode check | open | |
| C3 | GIVEN GET /dns/check?q=example.com THEN the same shape | open | |
| C4 | GIVEN domain 192.0.2.1 or an empty domain THEN 400 INVALID_INPUT | open | |
| C5 | GIVEN no DNS module THEN 503 UNAVAILABLE | open | |
| C6 | GIVEN a caller over the [api.dns] budget THEN 429 | open | |
| C7 | GIVEN Accept text/event-stream THEN the answer is still JSON | open | |
