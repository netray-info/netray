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
