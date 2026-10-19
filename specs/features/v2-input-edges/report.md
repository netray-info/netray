# Report: v2-input-edges

## Phase 1 — IDNA mapping in the domain rule

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 IDNA mapping: validate maps the trimmed input with UTS #46 after the raw address checks and returns the mapped ASCII form; a mapping error refuses with not a valid domain name | green | crates/common/tests/it/domain_idna.rs |
| C2 | R2 Mapped addresses: the address and numeric-last-label checks run again on the mapped form | green | crates/common/tests/it/domain_idna.rs |
| C3 | R3 Whitespace and forbidden characters inside are refused | green | crates/common/tests/it/domain_idna.rs |
| C4 | R4 One domain, one form: full-width, ideographic full stop, uppercase and Unicode labels map to one ASCII form | green | crates/common/tests/it/domain_idna.rs |
| C5 | R5 Every caller: lens API v1 runs, result page and evidence page see the mapped form | green | crates/lens/tests/it/idna_domain_routes.rs |
| C6 | GIVEN １.１.１.１ or １.１.１.１. WHEN validated THEN refused as an IP address | green | crates/common/tests/it/domain_idna.rs |
| C7 | GIVEN ０x７f.1 WHEN validated THEN refused as a numeric last label | green | crates/common/tests/it/domain_idna.rs |
| C8 | GIVEN whitespace, slash, percent or tab inside WHEN validated THEN refused with not a valid domain name | green | crates/common/tests/it/domain_idna.rs |
| C9 | GIVEN ＥＸＡＭＰＬＥ.com, example。com or Example.COM. WHEN validated THEN example.com | green | crates/common/tests/it/domain_idna.rs |
| C10 | GIVEN bücher.de or xn--bcher-kva.de WHEN validated THEN xn--bcher-kva.de | green | crates/common/tests/it/domain_idna.rs |
| C11 | GIVEN 123.example.com, 1password.com, localhost, _dmarc.example.com or an A-label name WHEN validated THEN unchanged | already_implemented | crates/common/tests/it/domain_idna.rs |
| C12 | GIVEN lens's app WHEN POST /api/v1/domains/１.１.１.１/runs THEN 400 DOMAIN_INVALID and no run recorded | green | crates/lens/tests/it/idna_domain_routes.rs |
| C13 | GIVEN lens's app WHEN GET /ＥＸＡＭＰＬＥ.com THEN 301 to /example.com | green | crates/lens/tests/it/idna_domain_routes.rs |
| C14 | GIVEN the evidence routes WHEN GET /１.１.１.１/dns THEN 404 | green | crates/web/tests/it/evidence_idna_domain.rs |

C7 (`０x７f.1`) was already refused before the change: its last label `1` is numeric on the raw
input; after the reader a second C7 test (`a.１２３`, `a.０x７f`) reaches the check on the mapped
form, and it is green. C11 is a regression guard. Both first tests are kept.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 (orchestrator) | orchestrator | n/a | n/a |

### Behavioural verification

`netray lens` built from this worktree, `crates/lens/lens.dev.toml`, bound to 127.0.0.1:48711:

```
$ curl -si -X POST 'http://127.0.0.1:48711/api/v1/domains/%EF%BC%91.%EF%BC%91.%EF%BC%91.%EF%BC%91/runs'
HTTP/1.1 400 Bad Request
{"error":{"code":"DOMAIN_INVALID","message":"invalid domain: IP addresses are not accepted as domain input: １.１.１.１"}}
$ curl -si 'http://127.0.0.1:48711/%EF%BC%A5%EF%BC%B8%EF%BC%A1%EF%BC%AD%EF%BC%B0%EF%BC%AC%EF%BC%A5.com'
HTTP/1.1 301 Moved Permanently
location: /example.com
$ curl -si -X POST 'http://127.0.0.1:48711/api/v1/domains/1.1.1.1%20./runs'
HTTP/1.1 400 Bad Request
{"error":{"code":"DOMAIN_INVALID","message":"invalid domain: not a valid domain name: 1.1.1.1 ."}}
```

### Reader

| class | where | finding | outcome |
|---|---|---|---|
| DEFERRED | crates/common/src/domain.rs | UTS #46 bidi rule refuses `_`/digit-led ASCII labels beside an RTL label | logged in the spec's decision log, left |
| AMENDMENT | spec.md decision log | a Unicode-keyed history is orphaned, not reachable under its A-label | decision reworded (affected phase 1, repaired) |
| NIT | crates/web/src/pages/result/mod.rs:164 | a hand-made `POST /bücher.de/run` gets the lower-case hint | left |
| NIT | crates/common/tests/it/domain_idna.rs | c7 never reached the mapped numeric check | second C7 test added |
| NIT | crates/common/tests/it/domain_idna.rs | rustfmt changed the protected file | commit carries `ADLC-Test-Change` |

`crates/web/tests/it/evidence_review.rs` `no_entry_page_link_is_percent_encoded` requested
`/%5Cevil.example.com/dns`; `\` is now refused by the rule, so the page never renders its link. The
test now uses `"`, which passes the rule and must be percent-encoded, keeping its intent.

## Phase 2 — Refuse a cost no wait can admit

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R6 DNS check_query_cost compares total and per-target cost with the bursts before the stream check and any charge; CostExceedsLimit names cost and limit and charges nothing | green | crates/dns/tests/jobs.rs |
| C2 | R7 prism V1 answers CostExceedsLimit 422 COST_EXCEEDS_LIMIT without Retry-After; API v1 DNS routes 400 INVALID_INPUT without Retry-After | green | crates/dns/tests/jobs_prism.rs |
| C3 | R8 TLS check_cost compares with both bursts before charging; remaining_budget is the smaller burst; the inspector refuses with CostExceedsLimit where not one address fits; the inspect page TLS job answers the message as its notice | green | crates/tls/tests/inspector.rs |
| C4 | R9 a request within every burst that finds too few tokens still answers 429 RATE_LIMITED with Retry-After | already_implemented | crates/web/tests/it/api_dns_cost_limit.rs |
| C5 | GIVEN a DNS limiter with global burst 20 WHEN DnsJobs::query runs eight types plus dnssec over three servers THEN CostExceedsLimit cost 30 limit 20 and the limiter still admits 20 | green | crates/dns/tests/jobs.rs |
| C6 | GIVEN a DNS limiter with per-target burst 5 WHEN DnsJobs::compare runs four types plus dnssec THEN CostExceedsLimit cost 6 limit 5 | green | crates/dns/tests/jobs.rs |
| C7 | GIVEN prism V1 defaults WHEN POST /api/compare with A AAAA MX dnssec over three servers THEN 422 COST_EXCEEDS_LIMIT naming 60 and 50, no retry-after | green | crates/dns/tests/jobs_prism.rs |
| C8 | GIVEN API v1 defaults WHEN POST /dns/compare with that body THEN 400 INVALID_INPUT naming 60 and 50, no retry-after | green | crates/web/tests/it/api_dns_cost_limit.rs |
| C9 | GIVEN API v1 global_burst 20 WHEN POST or GET query of cost 30 THEN 400 INVALID_INPUT naming 30 and 20, no retry-after | green | crates/web/tests/it/api_dns_cost_limit.rs |
| C10 | GIVEN API v1 global_burst 20 WHEN two queries of cost 18 from one client THEN the second answers 429 RATE_LIMITED with retry-after | already_implemented | crates/web/tests/it/api_dns_cost_limit.rs |
| C11 | GIVEN a TLS limiter with per-IP burst 2 WHEN check_cost with cost 3 THEN CostExceedsLimit cost 3 limit 2 and both limiters still admit 2 | green | crates/tls/tests/inspector.rs |
| C12 | GIVEN a TLS limiter with per-IP burst 10 and per-target burst 4 WHEN remaining_budget THEN 4 | green | crates/tls/tests/inspector.rs |
| C13 | GIVEN AppError::CostExceedsLimit WHEN rendered by tlsight THEN 422 COST_EXCEEDS_LIMIT without retry-after; mapped to TlsRefusal THEN InvalidInput | green | crates/tls/tests/inspector.rs |
| C14 | GIVEN API v1 per_ip_burst 2 for tls WHEN POST /tls/inspect with three ports THEN 400 INVALID_INPUT naming 3 and 2, no retry-after | green | crates/web/tests/it/api_tls_cost_limit.rs |
| C15 | GIVEN the inspect page with tls per_ip_burst 2 WHEN the TLS job runs three ports THEN no 429; the answer carries the cost message as its notice | green | crates/web/tests/it/api_tls_cost_limit.rs |

C4 and C10 (a spent budget still answers 429) already held; their test is kept as a guard.
C3's inspector path answers `CostExceedsLimit` from `check_cost`'s up-front comparison; the
cap-and-warn branch's own refusal is kept as a defence (the reader found it unreachable).
`crates/web/tests/it/api_dns_check.rs` `a_caller_over_the_dns_budget_is_429` pinned the bug itself
(cost 16 over a burst of 10 answered 429 forever); it now spends a burst of 18 with one check and
expects the second to be 429, its stated intent.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 DNS (error, refusal, limiter) | 1 (orchestrator) | orchestrator | n/a | n/a |
| G2 TLS (error, refusal, limiter, inspector) | 1 (orchestrator) | orchestrator | n/a | n/a |
| G3 web inspect-page TLS admission and start | 1 (orchestrator) | orchestrator | n/a | n/a |

### Reader

| class | where | finding | outcome |
|---|---|---|---|
| BLOCKER | crates/dns/src/security/rate_limit.rs | per-target burst compared for trace, dnssec and check, which charge no target | fixed: only charged targets compared; unit test `a_request_without_targets_is_not_held_to_the_per_target_burst`; R6 amended |
| DEFERRED | crates/dns/src/api/query.rs | a POST repeating a server is charged per listing, beyond the guard | fixed in the same pass: per-target cost times listings; unit test `a_repeated_target_counts_once_per_occurrence` |

### Behavioural verification

`netray lens` from this worktree (`crates/lens/lens.dev.toml`, 127.0.0.1:48711), API v1 defaults.
Before the change (binary from the previous build):

```
$ curl -si -X POST http://127.0.0.1:48711/api/v1/dns/compare -H 'content-type: application/json' -H 'accept: application/json' \
    -d '{"domain":"example.com","record_types":["A","AAAA","MX"],"dnssec":true,"servers":["cloudflare","google","quad9"]}'
HTTP/1.1 429 Too Many Requests
retry-after: 60
{"error":{"code":"RATE_LIMITED","message":"rate limit exceeded"}}
```

After:

```
HTTP/1.1 400 Bad Request
{"error":{"code":"INVALID_INPUT","message":"request cost 60 exceeds the limit of 50 per request"}}
$ curl ... -d '{"domain":"example.com","record_types":["A"],"servers":["cloudflare"]}'
HTTP/1.1 200 OK
```
