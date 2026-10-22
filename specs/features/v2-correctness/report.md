# Report: v2-correctness

## Phase 1 — Email: truncated answers and the SPF pass

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: the email resolver retries a truncated UDP answer once over TCP to the same server address and port (address, address with port, predefined provider); a failed retry names the TCP failure | green | crates/email/tests/it/resolver_tcp_retry.rs |
| C2 | R2: an SPF record with no Warn or Fail sub-check is a Pass with spf_ok whatever Info sub-checks it carries; email.spf is pass | green | crates/email/tests/spf_info_only.rs |
| C3 | GIVEN a responder answering TXT over UDP with TC and over TCP with a 3000-byte SPF record WHEN DnsResolver built with its address and port looks up TXT THEN it returns the SPF record | green | crates/email/tests/it/resolver_tcp_retry.rs |
| C4 | GIVEN that responder and a provider-like UDP config to it WHEN the TXT lookup runs THEN the TCP answer is returned and the responder saw one UDP and one TCP query | green | crates/email/tests/it/resolver_tcp_retry.rs |
| C5 | GIVEN a responder that sets TC over UDP and refuses TCP WHEN the TXT lookup runs THEN the error cause names the TCP failure | green | crates/email/tests/it/resolver_tcp_retry.rs |
| C6 | GIVEN a responder answering over UDP without TC WHEN the TXT lookup runs THEN no TCP query is made | already_implemented | crates/email/tests/it/resolver_tcp_retry.rs |
| C7 | GIVEN v=spf1 a:mail.example.com a:www.example.com -all with both names at 192.0.2.10 WHEN the SPF check runs THEN Pass with spf_ok and overlapping_cidrs (Info), email.spf pass | green | crates/email/tests/spf_info_only.rs |
| C8 | GIVEN overlapping prefixes and a ptr: mechanism WHEN the SPF check runs THEN Warn without spf_ok | already_implemented | crates/email/tests/spf_info_only.rs |
| C9 | GIVEN an SPF record with no Info, Warn or Fail sub-check WHEN the SPF check runs THEN Pass with spf_ok | already_implemented | crates/email/tests/spf_info_only.rs |

C6, C8 and C9 held before the change; their tests are kept as protection.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 3 | sonnet | 65006 | 123 |
| G2 | 1 | sonnet | 30422 | 27 |

### Review

- BLOCKER (fixed): `retries(0)` on the UDP group dropped hickory's one re-send for every failure, not only truncation. `lookup_on` now re-sends any non-truncated failure once on the same resolver.
- BLOCKER (fixed): the TCP twin kept `retries: 1`, so a failing retry sent two TCP queries; the twin now runs with `retries: 0`.
- NIT: no test covers the re-send of a non-truncated failure.

### Behavioural verification

skipped: a binary built in the worktree has no egress, and the live email path is the domain run; the loopback responder tests drive the real `DnsResolver` over UDP and TCP. The live recheck of amazon.com and github.com SPF runs after the last DNS phase.

## Phase 2 — DNS: the zone of a name below the apex

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R3: the zone is the name when its SOA is owned by it, else the closest ancestor (never single-label) with an SOA it owns; ancestor SOA lookups send no batch; an indefinite ancestor answer makes the zone checks unmeasured; all-negative keeps judging the name | green | crates/dns/src/api/check_zone_tests.rs |
| C2 | R4: below the apex, dns.ns, dns.ns_lame and dns.ns_delegation judge the zone; NS at the zone is sent as an NS batch; the dns.ns block holds the zone's NS | green | crates/dns/src/api/check_zone_tests.rs |
| C3 | R5: below the apex, the DNSSEC checks judge the zone's DNSKEY and DS, sent as batches; RRSIG/NSEC/NSEC3 stay the name's | green | crates/dns/src/api/check_zone_tests.rs |
| C4 | R6: no CAA at the name climbs ancestors (never single-label) to the first CAA RRset, sent as a CAA batch and judged; none keeps the warning; an indefinite ancestor answer makes dns.caa unmeasured | green | crates/dns/src/api/check_zone_tests.rs |
| C5 | GIVEN www.example.com without SOA and example.com with its SOA WHEN the zone is determined THEN example.com, and no SOA batch holds example.com's SOA | green | crates/dns/src/api/check_zone_tests.rs |
| C6 | GIVEN example.com with its own SOA WHEN the zone is determined THEN example.com and no ancestor is queried | already_implemented | crates/dns/src/api/check_zone_tests.rs |
| C7 | GIVEN a.b.example.com, b.example.com without SOA, example.com with SOA WHEN the zone is determined THEN example.com after two ancestor lookups, com never queried | green | crates/dns/src/api/check_zone_tests.rs |
| C8 | GIVEN www.example.com without SOA and example.com SOA SERVFAIL on every resolver WHEN the check runs THEN the three NS and three DNSSEC checks are unmeasured naming the example.com SOA lookup | green | crates/dns/src/api/check_zone_tests.rs |
| C9 | GIVEN www.example.com without NS and example.com NS ns1/ns2.example.net answering authoritatively WHEN the check runs THEN the NS checks pass, lame and consistency asked for example.com, the dns.ns block holds example.com's NS | green | crates/dns/src/api/check_zone_tests.rs |
| C10 | GIVEN www.example.com with RRSIG/NSEC but no DNSKEY/DS and example.com signed (alg 13 KSK, matching DS) WHEN the check runs THEN dnssec, dnskey_algorithm, dnssec_rollover pass and the dnskey/ds blocks hold example.com's records | green | crates/dns/src/api/check_zone_tests.rs |
| C11 | GIVEN www.example.com and example.com unsigned WHEN the check runs THEN dnssec warn "No DNSSEC records found", dnskey_algorithm and dnssec_rollover not_applicable "DNSSEC not deployed" | already_implemented | crates/dns/src/api/check_zone_tests.rs |
| C12 | GIVEN www.example.com without CAA and example.com with 0 issue "ca.example.net" WHEN the check runs THEN dns.caa judges it and the block holds it owned by example.com | green | crates/dns/src/api/check_zone_tests.rs |
| C13 | GIVEN a.b.example.com and b.example.com without CAA, example.com with CAA WHEN the check runs THEN CAA was looked up at b.example.com and example.com, never com | green | crates/dns/src/api/check_zone_tests.rs |
| C14 | GIVEN no CAA on www.example.com and every ancestor to example.com WHEN the check runs THEN dns.caa warn "No CAA records found" | already_implemented | crates/dns/src/api/check_zone_tests.rs |
| C15 | GIVEN no CAA at www.example.com and example.com's CAA lookup failing on every resolver WHEN the check runs THEN dns.caa unmeasured naming the example.com CAA lookup | green | crates/dns/src/api/check_zone_tests.rs |
| C16 | GIVEN example.com, a zone apex, WHEN the check runs THEN checks and lookups equal today's (no ancestor lookup) | already_implemented | crates/dns/src/api/check_zone_tests.rs |
| C17 | GIVEN www.xn--bcher-kva.example without SOA and xn--bcher-kva.example owning an SOA WHEN the zone is determined THEN it is xn--bcher-kva.example | green | crates/dns/src/api/check_zone_tests.rs |
| C18 | GIVEN www.example.com CNAME example.net (NS, DNSKEY, DS of example.net answered at the name) and zone example.com WHEN the check runs THEN NS checks and DNSSEC lints read only example.com's records and raw NS queries go only to example.com's servers | green | crates/dns/src/api/check_zone_tests.rs |
| C19 | GIVEN www.example.com whose own SOA lookup SERVFAILs everywhere WHEN the check runs THEN no ancestor is queried | green | crates/dns/src/api/check_zone_tests.rs |
| C20 | GIVEN unregistered.example.co.uk NXDOMAIN everywhere and co.uk owning an SOA WHEN the check runs THEN co.uk is never queried and the NS checks judge the name | green | crates/dns/src/api/check_zone_tests.rs |
| C21 | GIVEN a ten-label name below example.com and no SOA below example.com WHEN the zone is determined THEN at most 8 ancestors are queried | green | crates/dns/src/api/check_zone_tests.rs |

C6, C11, C14 and C16 held before the change. Two fixture rows were corrected during the phase (`ADLC-Test-Change`): the RRSIG's key tag, and c12's Pass, since mhost's CAA lint warns on a lone `issue` record.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 (7 of 9 green; 2 fixture faults) | opus | 121219 | 298 |
| G1 review fixes | 1 | opus | n/a | n/a |

### Review

- BLOCKER (fixed, c17): owner comparison used hickory's Unicode rendering; IDN zones were never found.
- BLOCKER (fixed, c18): the name's NS, DNSKEY and DS (a CNAME target's) stayed beside the zone's; the zone's now replace them.
- AMENDMENT (repaired in phase, c19): a failed SOA lookup at the name started a climb; it no longer does.
- AMENDMENT (repaired in phase, c20): the climb could reach a public suffix's zone; it stops at the registrable domain.
- AMENDMENT (repaired in phase, c21): the climb was bounded only by the label count, after the cost was charged; it stops after 8 ancestors.
- DEFERRED (repaired): the zone phase ran outside the stream deadline; it is now bounded by it.
- DEFERRED: a failed NS, DNSKEY or DS batch at the name still makes the zone checks unmeasured when the zone's lookups answered (conservative under Rule M).
- NIT: a three-label apex without CAA climbs CAA once, as RFC 8659 asks.

### Behavioural verification

skipped here: a worktree binary has no egress; the live rerun of docs.github.com, www.ietf.org and www.isc.org runs after Phase 3 with the binary copied to main's path.

## Phase 3 — DNS: TTLs from the authoritative servers

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R7: a record whose highest recursive TTL is below 60 s is judged on its authoritative TTL (looked up at the zone's nameservers, sent as a batch of its type so the block shows the highest TTL); authoritative 60 s or more is not low; no authoritative answer keeps the recursive TTL | green | crates/dns/src/api/check_ttl_tests.rs |
| C2 | GIVEN example.com A 192.0.2.10 at TTL 29 from resolvers and 60 from the authoritative server WHEN the check runs THEN dns.ttl pass and the dns.a block shows TTL 60 | green | crates/dns/src/api/check_ttl_tests.rs |
| C3 | GIVEN TTL 29 from resolvers and 30 from the authoritative server WHEN the check runs THEN dns.ttl warn naming example.com. with 30s | green | crates/dns/src/api/check_ttl_tests.rs |
| C4 | GIVEN resolver TTL 29 and no authoritative server answering WHEN the check runs THEN dns.ttl warn with 29s | already_implemented | crates/dns/src/api/check_ttl_tests.rs |
| C5 | GIVEN every resolver TTL at 60 s or more WHEN the check runs THEN no authoritative TTL lookup is made | already_implemented | crates/dns/src/api/check_ttl_tests.rs |
| C6 | GIVEN resolvers answering 192.0.2.10 (TTL 60) and 192.0.2.11 (TTL 29) and the authoritative server 192.0.2.10 alone (TTL 60) WHEN the check runs THEN dns.ttl pass | green | crates/dns/src/api/check_ttl_tests.rs |
| C7 | GIVEN xn--bcher-kva.example A TTL 29 from resolvers and 300 authoritative WHEN the check runs THEN dns.ttl pass | green | crates/dns/src/api/check_ttl_tests.rs |
| C8 | GIVEN www.example.com CNAME web.example.com TTL 29, only in the A lookup, and 300 authoritative WHEN the check runs THEN dns.ttl pass | green | crates/dns/src/api/check_ttl_tests.rs |
| C9 | GIVEN www.example.com CNAME shop.example.net with A TTL 20 WHEN the check runs THEN no authoritative lookup for shop.example.net and dns.ttl warns with 20s | already_implemented | crates/dns/src/api/check_ttl_tests.rs |

C4, C5 and C9 held before the change (C9 guards that a CNAME target outside the zone is never re-read).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 3 (green after one unrun one-line fix, confirmed by my run) | opus | 110225 | 485 |

### Review

- BLOCKER (fixed, c7): `with_ttl` compared serialised (Unicode) owner names with the A-label pair; names now compare as A-labels.
- BLOCKER (fixed, c8): only lookups of the record's own query type were searched; a CNAME kept in the A lookup, or glue in the NS lookup, got no batch.
- AMENDMENT (repaired in phase, c9): owners outside the zone (CNAME targets) were re-read at the zone's servers; only names at or below the zone are now.
- Found live (fixed, c6): a rotating address set kept a member's cached TTL when the authoritative answer listed other addresses; the authoritative TTL now covers the RRset (RFC 2181 §5.2).
- NIT: when the authoritative TTL is below a resolver's (TTL floor), the finding names the higher value; the verdict is the same.

### Behavioural verification

Worktree binary copied to main's path (backed up, restored, `cmp` identical), one lens instance on 127.0.0.1:8796 with fresh history and the DNS per-target limit raised (the dev limit 50/19 admits two runs, inventory 10.20). `POST /api/v1/domains/<d>/runs`:

```
github.com      B  dns.ttl pass (was warn 29s)  dns.ns pass  dns.caa pass  email.spf pass (was unmeasured, truncated)
docs.github.com C  dns.ns/ns_lame/ns_delegation pass (were fail)  dns.caa pass (was warn)  dns.ttl warn 30s (authoritative TTL is 30)
www.isc.org     B  dns.dnssec, dnskey_algorithm, dnssec_rollover pass (were warn / n/a)  dns.caa pass  dns.ttl pass
www.ietf.org    C  dns.dnssec, dnskey_algorithm, dnssec_rollover pass (was pass beside "not deployed")
amazon.com      B  email.spf warn "MX IP(s) not in SPF" (was unmeasured, truncated)
lwn.net         B  email.spf pass (was not_applicable)
nginx.org       incomplete: dns.ttl pass (was warn 52s); tls.legacy_versions unmeasured "no answer to the SSLv3 probe (alert unrecognized_name)", outside this feature
```

Evidence: netray.info `specs/inventory/evidence/correctness/1022/fixed/`.
