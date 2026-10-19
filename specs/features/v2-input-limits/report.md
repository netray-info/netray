# Report: v2-input-limits

## Phase 1 — Address forms in the domain rule

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: validate refuses an address with one trailing dot or in square brackets, with the existing IP message | green | crates/common/tests/it/domain_address_forms.rs |
| C2 | R2: validate refuses a name whose last label is a number (digits, or 0x hex) | green | crates/common/tests/it/domain_address_forms.rs |
| C3 | R3: every caller refuses with its existing invalid-domain answer | green | crates/lens/tests/it/address_domain_routes.rs |
| C4 | GIVEN 192.0.2.1. or 2001:db8::1. or ::1. or [2001:db8::1] or [::1] WHEN validated THEN the IP message | green | crates/common/tests/it/domain_address_forms.rs |
| C5 | GIVEN 1.2.3, 3221225985, 0x7f.1, example.0x1F, 192.0.2.01 or example.123. WHEN validated THEN the numeric-label message | green | crates/common/tests/it/domain_address_forms.rs |
| C6 | GIVEN 123.example.com, 1password.com, Example.COM., localhost, a1 or x.0xg WHEN validated THEN accepted as before | green | crates/common/tests/it/domain_address_forms.rs |
| C7 | GIVEN lens WHEN POST /api/v1/domains/192.0.2.1./runs THEN 400 DOMAIN_INVALID, no run recorded | green | crates/lens/tests/it/address_domain_routes.rs |
| C8 | GIVEN lens WHEN GET /api/v1/domains/192.0.2.1. THEN 400 DOMAIN_INVALID | green | crates/lens/tests/it/address_domain_routes.rs |
| C9 | GIVEN lens WHEN GET /192.0.2.1. THEN 400 refusal page, no location | green | crates/lens/tests/it/address_domain_routes.rs |
| C10 | GIVEN lens WHEN GET /badge/192.0.2.1..svg THEN 400 DOMAIN_INVALID | green | crates/lens/tests/it/address_domain_routes.rs |
| C11 | GIVEN the evidence routes WHEN GET /192.0.2.1./dns THEN 404 | green | crates/web/tests/it/evidence_address_domain.rs |

Tests written by the orchestrator directly (one production file, eleven small criteria); RED
confirmed per test before the fix: the rule accepted every form, lens answered 404/301/200/badge SVG.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| 1 | 1 (orchestrator) | orchestrator | n/a | n/a |

### Review

- BLOCKER (fixed, second pass green): `[::1].` and `[2001:db8::1].` passed, since the bracket strip
  ran before the dot strip; brackets now come off the undotted input, and C4 carries both cases.
- DEFERRED: inner whitespace and full-width digits (`1.1.1.1 .`, `１.１.１.１`) still pass the rule
  and the HTTP module reads them as IPv4; a character-set rule is a non-goal of this spec.

### Behavioural verification

`netray lens` built from this branch, production fixture config on 127.0.0.1:18382 (ip module
removed: no GeoIP data locally):

```
$ curl -s -X POST -H 'accept: application/json' http://127.0.0.1:18382/api/v1/domains/192.0.2.1./runs
400 {"error":{"code":"DOMAIN_INVALID","message":"invalid domain: IP addresses are not accepted as domain input: 192.0.2.1."}}
$ curl -s -X GET http://127.0.0.1:18382/api/v1/domains/192.0.2.1.
400 {"error":{"code":"DOMAIN_INVALID",...}}
$ curl -s -X GET http://127.0.0.1:18382/192.0.2.1.
400 location= (refusal page "Not a domain name - netray")
$ curl -s -X GET http://127.0.0.1:18382/badge/192.0.2.1..svg
400 {"error":{"code":"DOMAIN_INVALID",...}}
$ curl -s -X GET http://127.0.0.1:18382/api/v1/domains/1.2.3
400 {"error":{"code":"DOMAIN_INVALID","message":"invalid domain: the last label is a number, so the name reads as an IP address: 1.2.3"}}
```

## Phase 2 — DNSSEC types inside the limit

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R4: with +dnssec, DNSKEY and DS are added before the record-type limit and the cost in query and compare | green | crates/dns/tests/jobs.rs |
| C2 | R5: eight types plus +dnssec run; types already holding DNSKEY and DS are not counted twice | green | crates/dns/tests/jobs.rs |
| C3 | GIVEN ten types and dnssec WHEN DnsJobs::query runs THEN TooManyRecordTypes requested 12 max 10 | green | crates/dns/tests/jobs.rs |
| C4 | GIVEN ten types and dnssec WHEN DnsJobs::compare runs THEN the same refusal | green | crates/dns/tests/jobs.rs |
| C5 | GIVEN ten types and +dnssec in the query language WHEN query_parsed runs THEN the same refusal | green | crates/dns/tests/jobs.rs |
| C6 | GIVEN prism V1 GET /api/query with ten types and +dnssec WHEN answered THEN 422 TOO_MANY_RECORD_TYPES | green | crates/dns/tests/jobs_prism.rs |
| C7 | GIVEN API v1 POST or GET /dns/query with ten types and +dnssec WHEN answered THEN 400 with the eleventh-type code | green | crates/web/tests/it/api_dns_dnssec_limit.rs |
| C8 | GIVEN eight types and dnssec WHEN DnsJobs::query runs THEN ten batches | green | crates/dns/tests/jobs.rs |
| C9 | GIVEN ten types including DNSKEY and DS and dnssec WHEN DnsJobs::query runs THEN the job starts | green | crates/dns/tests/jobs.rs |

Tests written by the orchestrator directly; RED confirmed (every job started, the routes answered
200). C2's no-double-count half and C8, C9 passed before the fix and protect what exists. C6's
first draft expected 400; prism answers the limit with 422, so the test follows the existing refusal
(spec scenario amended, Decision log).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| 1 | 1 (orchestrator) | orchestrator | n/a | n/a |

### Review

- No blockers.
- DEFERRED: compare now charges the added DNSKEY and DS (R4), so a compare over three servers with
  `+dnssec` (6 types x 3 servers x 4 transports = 72) exceeds the default per-IP burst of 64 and
  gets the pre-existing permanent 429 with `Retry-After: 60` (`crates/common/src/rate_limit.rs`,
  the InsufficientCapacity case). The refusal is intended; its retryable form predates this change.

### Behavioural verification

`netray lens` built from this branch on 127.0.0.1:18382 (the refusal comes before any lookup, so
the local DNS block does not matter):

```
$ curl -s -X POST -H 'content-type: application/json' -d '{"domain":"example.com","record_types":["A","AAAA","MX","TXT","NS","SOA","CAA","CNAME","PTR","SRV"],"dnssec":true}' http://127.0.0.1:18382/api/v1/dns/query
{"error":{"code":"INVALID_INPUT","message":"too many record types: 12 exceeds limit of 10"}}
400
$ curl -s 'http://127.0.0.1:18382/api/v1/dns/query?q=example.com%20A%20AAAA%20MX%20TXT%20NS%20SOA%20CAA%20CNAME%20PTR%20SRV%20%2Bdnssec'
{"error":{"code":"INVALID_INPUT","message":"too many record types: 12 exceeds limit of 10"}}
400
```
