# Spec: v2-correctness

Status: Ready for Implementation
Created: 2026-10-22

## Goal

V2's verdicts are as right as V1's, check by check, on real domains, and where both were wrong,
V2 is now right. The 2026-10-22 rerun of inventory area 10 (private:
`netray.info/specs/inventory/sections/10-correctness.md`, evidence `evidence/correctness/1022/`)
compared 530 check pairs over 10 public domains: 513 agree, 17 differ. This feature closes every
row where V2 is wrong: the email resolver drops truncated answers (10.4, 10.5), an Info line
suppresses an SPF pass (10.10), DNS checks judge a name below a zone apex as if it were the apex
(NS, DNSSEC, CAA: 10.11 to 10.14), the TTL check judges cached TTLs (10.15 to 10.17), an IP query
in the DNS inspector never reverses the address (2.10), `@system` crashes its job (2.13), plus
two reproduced findings: a section whose every check is unmeasured reports `measured`, and a
single-label name in an OG card URL starts a domain run.

## Non-goals

- No scoring change: no profile edit, no weight, no hard fail, no catalogue id. Whether a missing
  HTTPS record should fail (10.19) and how the DNS per-target limiter should treat domain runs
  (10.20) are operator proposals, outside this spec.
- No template, stylesheet or island change (lane A owns them).
- No V1 removal. The DNS and email code is shared, so V1's answers improve with V2's.
- `tls.caa_compliant` keeps its own CAA source; only `dns.caa` changes.
- Rows where V1 was wrong and V2 right (10.7 referrer-policy list, 10.8, 10.9) and environment
  rows (10.6 amazon.com's bot page, 10.18) need no work.

## Context and constraints

- DNS checks are computed in `netray_dns::api::check::run_check` (`crates/dns/src/api/check.rs`):
  phase 1 fans out one lookup per record type at the queried name over the configured resolvers
  and sends a `CheckEvent::Batch` per type; phase 1.75 runs the lame-delegation and
  delegation-consistency checks against the nameservers (`RawOutbound`, RD=0); phase 2 runs the
  mhost lints (`lint_lookups`) over the unique records. `netray_dns::module::translate` turns the
  events into `dns.*` checks; a records block (`records_block`) merges every answered batch of its
  type, keeping each record (owner, type, data) once with its highest TTL, so a batch at another
  owner shows under its own owner. A batch whose lookups all failed makes the checks reading that
  type `unmeasured` (Rule M).
- The prism `+check` job and the domain run share `run_check`; the change applies to both.
- `cname_apex` decides apex-ness from the SOA batch: a zone-discovery SOA lookup at an ancestor
  must not reach that batch.
- RFC 8659 §3: the relevant CAA RRset of a name is the first one found climbing from the name
  towards the root.
- The email resolver (`crates/email/src/dns/resolver.rs`) turns each nameserver config into its own
  mhost `Resolver` and rotates over them; a UDP answer with the TC bit surfaces as
  `response was truncated; TCP already tried or no other transport available`.
- Beacon's SPF check (`crates/email/src/checks/spf.rs`) adds `spf_ok` only to an empty sub-check
  list; `netray_email::module::fold_category` makes an Info-only category `not_applicable`.
- Evidence rules (SDD §3.7, CLAUDE.md): the block a judged check references holds the fact that
  decided it; no placeholders.
- Tests run offline: fixtures use the example.com family and documentation address ranges
  (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24, 2001:db8::/32); a network seam is a stub
  (an in-process DNS responder on 127.0.0.1, a stub `RawOutbound` or a lookup seam). The live
  domains of the inventory stay out of committed files.

## Requirements

1. The email resolver retries a lookup whose UDP answer was truncated once over TCP to the same
   server address and port, and that answer is the lookup's answer. This holds for nameservers
   given as an address, an address with port, or a predefined provider. When the TCP retry fails
   too, the lookup error names the TCP failure.
2. An SPF record with no Warn or Fail sub-check is a Pass (`spf_ok`, "valid SPF with N authorized
   prefix(es)"), whatever Info sub-checks it carries (`overlapping_cidrs` among them); the Info
   sub-checks stay in the category's sub-checks. `email.spf` is then `pass`.
3. `run_check` determines the queried name's zone: the name is a zone apex when its SOA lookup
   answers an SOA record owned by the name (owners compared as A-labels). When the name's own SOA
   lookup has no definite answer, nothing is climbed and the checks judge the name as before. Otherwise the zone is the closest ancestor, dropping
   one label at a time, never above the name's registrable domain (public suffix list; never a
   single-label name when the suffix is unknown) and at most 8 ancestors, whose SOA lookup answers
   an SOA record owned by it. The climb ends at the stream deadline; a climb cut off there leaves
   the zone unknown. Ancestor SOA lookups are sent as no batch and never reach the SOA batch.
   When an ancestor's SOA lookup gets no definite answer before the zone is found, the zone is
   unknown and the checks of requirements 4 and 5 are `unmeasured` naming the failed lookup. When
   no ancestor is a zone apex (every answer a definite negative), the checks judge the queried
   name as before.
4. For a name below its zone's apex, `dns.ns`, `dns.ns_lame` and `dns.ns_delegation` judge the
   zone: NS is looked up at the zone over the configured resolvers and sent as an `NS` batch, and
   the lame-delegation and delegation-consistency checks query the zone's nameservers for the zone.
   The zone's NS, DNSKEY and DS lookups replace those at the queried name in what the checks read
   (a name with a CNAME answers the target's records there).
   The `dns.ns` block then holds the zone's NS records under the zone's owner name.
5. For a name below its zone's apex, `dns.dnssec`, `dns.dnskey_algorithm` and
   `dns.dnssec_rollover` judge the zone's DNSKEY and DS: both are looked up at the zone and sent
   as `DNSKEY` and `DS` batches; RRSIG, NSEC and NSEC3 stay those of the queried name. A name in a
   signed zone is then never "DNSSEC not deployed", and `dns.dnssec` never warns "RRSIG present but
   no DNSKEY records found" there.
6. When the CAA lookup at the queried name answers no CAA record, `run_check` looks up CAA at each
   ancestor in turn, with the bounds of requirement 3, until one answers CAA records; that RRset is
   sent as a `CAA` batch and judged by `dns.caa`, whose block then holds it under the ancestor's
   owner name. No CAA anywhere keeps "No CAA records found". An ancestor lookup with no definite
   answer before an RRset is found makes `dns.caa` `unmeasured` naming it.
7. `dns.ttl` judges a record whose highest recursive TTL is below 60 s on its TTL as served by the
   zone's authoritative nameservers: the record's name and type are looked up at those servers
   and the answer is sent as a batch of that type, so the records block shows the highest TTL
   across all answers, the authoritative one included; the authoritative TTL applies to the whole
   RRset (RFC 2181 §5.2), and to the RRSIGs covering it (RFC 4034 §3), which are never re-read on
   their own. Only names at or below the zone are re-read. A record whose
   authoritative TTL is 60 s or more is not low. When no authoritative server answers, the recursive TTL is judged, as
   today.
8. A DNS inspector query whose domain is an IP address queries PTR at the address's reverse name:
   `d.c.b.a.in-addr.arpa.` for an IPv4 address, the nibble form under `ip6.arpa.` for an IPv6
   address. The batch's lookups name the reverse name.
9. A DNS job's resolver group carries one circuit-breaker key per resolver: `@system` adds one key
   per system nameserver, so a query, compare, authcompare or check job with `@system` (where the
   configuration allows it) answers its batches and its `done` event instead of ending empty.
10. A section that the module reports measured with at least one check, every check
    `unmeasured`, is reported with outcome `incomplete` and the reason "every check unmeasured:
    <the first check's first finding>". Its checks keep their statuses, findings and evidence,
    and the grade is unchanged (already incomplete). This holds for the API's run answer and
    section events, the result page's live and stored views, and the stored run.
11. No domain run starts for a single-label name: the API (`POST /api/v1/domains/{domain}/runs`
    and the reads that start runs), the badge and the OG card refuse it as an invalid domain
    before admission, with the message "a domain needs at least two labels"; the result page's
    router keeps treating a segment without a dot as no domain (its fallback), which starts no
    run. The DNS inspector keeps accepting single-label names (`com NS`).
12. SDD `specs/sdd/v2.md` states requirements 3 to 7 in §3.9's DNS rows and requirement 10 and 11
    where it describes section outcomes and domain input, with a decision row, in the same commit
    as the code; `CHANGELOG.md` `[Unreleased]` names the verdict changes (subdomains, SPF, TTL,
    truncated email lookups).

## Phase 1 — Email: truncated answers and the SPF pass

**Depends on:** none
**Requirements:** 1, 2

`crates/email/src/dns/resolver.rs`, `crates/email/src/checks/spf.rs`; tests in
`crates/email/tests/` (an in-process DNS responder on 127.0.0.1 answering UDP with TC set and TCP
in full) and the SPF check tests over `TestDnsResolver`.

### Test Scenarios

GIVEN a responder on 127.0.0.1 that answers a TXT query for example.com over UDP with the TC bit and no records, and over TCP with a 3000-byte SPF record WHEN `DnsResolver` built with that address and port looks up the TXT records of example.com THEN it returns the SPF record.
GIVEN that responder and a resolver built with a predefined-provider-like UDP config to it WHEN the TXT lookup runs THEN the TCP answer is returned and the responder saw one UDP and one TCP query.
GIVEN a responder that sets TC over UDP and refuses TCP connections WHEN the TXT lookup runs THEN the lookup is an error whose cause names the TCP failure, never the truncation alone.
GIVEN a responder that answers over UDP without TC WHEN the TXT lookup runs THEN no TCP query is made.
GIVEN the SPF record `v=spf1 a:mail.example.com a:www.example.com -all` where both names resolve to 192.0.2.10 WHEN the SPF check runs THEN the category verdict is Pass, it carries `spf_ok` and `overlapping_cidrs` (Info), and `email.spf` is `pass`.
GIVEN an SPF record with overlapping prefixes and a `ptr:` mechanism WHEN the SPF check runs THEN the verdict is Warn and `spf_ok` is absent.
GIVEN an SPF record with no Info, Warn or Fail sub-check WHEN the SPF check runs THEN the verdict is Pass with `spf_ok`, as before.

## Phase 2 — DNS: the zone of a name below the apex

**Depends on:** none
**Requirements:** 3, 4, 5, 6

`crates/dns/src/api/check.rs` (`run_check`, zone discovery, CAA climb), `crates/dns/src/module.rs`
if `translate` needs it; tests drive `run_check` through a lookup seam or an in-process responder,
with a stub `RawOutbound` for the nameserver checks.

### Test Scenarios

GIVEN www.example.com with no SOA (NODATA) and example.com with an SOA owned by example.com WHEN the zone is determined THEN it is example.com, and no SOA batch holds example.com's SOA.
GIVEN example.com whose SOA lookup answers an SOA owned by example.com WHEN the zone is determined THEN it is example.com and no ancestor is queried.
GIVEN a.b.example.com, b.example.com without SOA and example.com with SOA WHEN the zone is determined THEN it is example.com after two ancestor lookups, and com is never queried.
GIVEN www.example.com without SOA and example.com's SOA lookup failing with SERVFAIL on every resolver WHEN the check runs THEN `dns.ns`, `dns.ns_lame`, `dns.ns_delegation`, `dns.dnssec`, `dns.dnskey_algorithm` and `dns.dnssec_rollover` are `unmeasured` with a finding naming the example.com SOA lookup.
GIVEN www.example.com with no NS (NODATA) and example.com with NS ns1.example.net and ns2.example.net answering authoritatively WHEN the check runs THEN `dns.ns`, `dns.ns_lame` and `dns.ns_delegation` are `pass`, the lame and consistency queries asked for example.com, and the `dns.ns` block holds the two NS records owned by example.com.
GIVEN www.example.com with RRSIG and NSEC but no DNSKEY or DS, and example.com with a DNSKEY (algorithm 13, KSK) and a matching DS WHEN the check runs THEN `dns.dnssec` is `pass`, `dns.dnskey_algorithm` and `dns.dnssec_rollover` are `pass`, and the `dns.dnskey` and `dns.ds` blocks hold example.com's records.
GIVEN www.example.com and example.com both unsigned WHEN the check runs THEN `dns.dnssec` is `warn` "No DNSSEC records found" and `dns.dnskey_algorithm` and `dns.dnssec_rollover` are `not_applicable` "DNSSEC not deployed", as for an apex.
GIVEN www.example.com with no CAA and example.com with `0 issue "ca.example.net"` WHEN the check runs THEN `dns.caa` judges that record (no "No CAA records found" warning) and the `dns.caa` block holds it owned by example.com.
GIVEN a.b.example.com with no CAA, b.example.com with no CAA and example.com with CAA WHEN the check runs THEN CAA was looked up at b.example.com and example.com, never at com.
GIVEN www.example.com and every ancestor down to example.com without CAA WHEN the check runs THEN `dns.caa` is `warn` "No CAA records found", as today.
GIVEN www.example.com without CAA and the CAA lookup of example.com failing on every resolver WHEN the check runs THEN `dns.caa` is `unmeasured` naming the example.com CAA lookup.
GIVEN example.com, a zone apex, WHEN the check runs THEN the checks and the lookups made equal today's (no ancestor lookup).
GIVEN www.xn--bcher-kva.example with no SOA and xn--bcher-kva.example answering an SOA it owns WHEN the zone is determined THEN it is xn--bcher-kva.example.
GIVEN www.example.com with a CNAME to example.net, whose NS, DNSKEY and DS answer at the name, and example.com as the zone WHEN the check runs THEN the NS checks and the DNSSEC lints read only example.com's NS, DNSKEY and DS, and the raw NS queries go to example.com's nameservers only.
GIVEN www.example.com whose own SOA lookup fails with SERVFAIL on every resolver WHEN the check runs THEN no ancestor is queried.
GIVEN unregistered.example.co.uk (registrable domain example.co.uk) answering NXDOMAIN everywhere and co.uk owning an SOA WHEN the check runs THEN co.uk is never queried and the NS checks judge the name as before.
GIVEN a name with ten labels below example.com and no SOA anywhere below example.com WHEN the zone is determined THEN at most 8 ancestors are queried.
GIVEN www.example.com whose own NS lookup fails with SERVFAIL on every resolver, and example.com as its zone answering NS WHEN the check runs THEN `dns.ns`, `dns.ns_lame` and `dns.ns_delegation` judge example.com and are not `unmeasured`.
GIVEN www.example.com without CAA and the CAA climb still running at the stream deadline WHEN the check runs THEN `dns.caa` is `unmeasured` naming the deadline.

## Phase 3 — DNS: TTLs from the authoritative servers

**Depends on:** Phase 2
**Requirements:** 7

`crates/dns/src/api/check.rs`; tests as Phase 2.

### Test Scenarios

GIVEN example.com A 192.0.2.10 answered by the resolvers with TTL 29 and by the zone's authoritative server with TTL 60 WHEN the check runs THEN `dns.ttl` is `pass` and the `dns.a` block shows TTL 60.
GIVEN the same record answered with TTL 29 by the resolvers and TTL 30 by the authoritative server WHEN the check runs THEN `dns.ttl` is `warn` naming example.com with 30s.
GIVEN a resolver TTL of 29 and no authoritative server answering WHEN the check runs THEN `dns.ttl` is `warn` with 29s, as today.
GIVEN every resolver TTL at 60 s or more WHEN the check runs THEN no authoritative TTL lookup is made.
GIVEN example.com A answered by the resolvers as 192.0.2.10 (TTL 60) and 192.0.2.11 (TTL 29) and by the authoritative server as 192.0.2.10 alone (TTL 60) WHEN the check runs THEN `dns.ttl` is `pass`: the TTL belongs to the RRset.
GIVEN xn--bcher-kva.example A at TTL 29 from the resolvers and 300 from its authoritative server WHEN the check runs THEN `dns.ttl` is `pass`.
GIVEN www.example.com CNAME web.example.com at TTL 29 from the resolvers, carried only in the A lookup, and 300 from the authoritative server WHEN the check runs THEN `dns.ttl` is `pass`.
GIVEN www.example.com CNAME shop.example.net whose A is TTL 20 from the resolvers WHEN the check runs THEN no authoritative lookup is made for shop.example.net and `dns.ttl` warns with 20s.
GIVEN example.com's RRSIG records at TTL 29 from the resolvers WHEN the check runs THEN no authoritative lookup is made for RRSIG: an RRSIG carries the TTL of the RRset it covers (RFC 4034 §3).
GIVEN example.com A and its RRSIG at TTL 29 from the resolvers and A at 60 from the authoritative server WHEN the check runs THEN `dns.ttl` is `pass`: the RRSIG takes the covered RRset's TTL.

## Phase 4 — DNS inspector: reverse names and `@system`

**Depends on:** none
**Requirements:** 8, 9

`crates/dns/src/parser.rs` or the query job, `crates/dns/src/api/query.rs`
(`build_resolver_group`); tests in `crates/dns/tests/it/` and unit tests.

### Test Scenarios

GIVEN the query `192.0.2.1` WHEN it is parsed into a job THEN the PTR lookup is made for `1.2.0.192.in-addr.arpa.`.
GIVEN the query `2001:db8::1` WHEN it is parsed into a job THEN the PTR lookup is made for the nibble name ending in `8.b.d.0.1.0.0.2.ip6.arpa.`.
GIVEN the query `example.com PTR` WHEN parsed THEN the lookup name stays example.com.
GIVEN a configuration allowing system resolvers and a system configuration with two nameservers WHEN the resolver group for `@system` is built THEN it holds as many breaker keys as resolvers.
GIVEN a query job over a resolver group with more resolvers than the system entry WHEN the job runs THEN it sends its batches and a `done` event and does not panic.

## Phase 5 — Section outcome and domain input

**Depends on:** none
**Requirements:** 10, 11, 12

`crates/engine` or the shared mapping from `SectionOutcome` to `RunSection` used by
`crates/web/src/api/runs.rs`, `crates/web/src/pages/result/views.rs` and the run store;
`crates/lens/src/input.rs` (`validate_domain`) and the entries that start runs; SDD; CHANGELOG.

### Test Scenarios

GIVEN an HTTP module stub whose section has 13 checks, all `unmeasured`, the first finding "HTTPS probe: timed out after 10s" WHEN a run is answered by `POST /api/v1/domains/example.com/runs` THEN the http section's outcome is `incomplete` with reason "every check unmeasured: HTTPS probe: timed out after 10s", the 13 checks are listed `unmeasured` with their findings, and the grade is `incomplete`.
GIVEN the same stub WHEN the run streams with `Accept: text/event-stream` THEN the http `section` event carries outcome `incomplete`, that reason and the 13 checks.
GIVEN a section with one `pass` check and the rest `unmeasured` WHEN the run is answered THEN its outcome stays `measured`.
GIVEN a measured section with no checks WHEN the run is answered THEN its outcome stays `measured`.
GIVEN `POST /api/v1/domains/landing/runs` WHEN answered THEN it is `400` (the invalid-domain refusal) with "a domain needs at least two labels" and no run starts.
GIVEN `GET /og/landing.png` WHEN answered THEN it is refused as an invalid domain and no run starts; GIVEN `GET /badge/landing.svg` THEN the same.
GIVEN the result page for `landing` WHEN requested THEN no run starts.
GIVEN `example.com` and `www.example.com` WHEN validated for a domain run THEN both are accepted.
GIVEN the DNS inspector query `com NS` WHEN parsed THEN it is accepted.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate (operator,
  2026-10-19, extended 2026-10-22).
- Independent reading of the Draft skipped: the review step reads the whole range against the spec
  (implementer under the operator's autonomy mandate, 2026-10-22).
- A name below its zone's apex is judged by its zone for NS, DNSSEC and CAA, not marked not
  applicable: the zone's nameservers and keys are the ones that serve and sign the name, so their
  faults are the name's faults, and RFC 8659 defines CAA by the climb. Consequence: subdomains no
  longer fail 11 weight points of NS checks; a subdomain inherits its zone's verdicts and evidence
  (implementer under the operator's autonomy mandate, 2026-10-22).
- Zone discovery uses SOA ownership, one SOA lookup per ancestor, and never queries a
  single-label name: an authority section is not exposed by mhost's lookups, and TLD records say
  nothing about the domain (implementer under the operator's autonomy mandate, 2026-10-22).
- Review of phase 2 (implementer under the operator's autonomy mandate, 2026-10-22): the climb
  stops at the registrable domain and after 8 ancestors (a registry's zone says nothing about an
  unregistered name, and a 120-label name must not buy 240 uncharged lookups); a failed SOA lookup
  at the name itself never starts a climb (Rule M); the zone's NS, DNSKEY and DS replace the name's,
  which for a CNAME are the target's; owners compare as A-labels (IDN zones); the climb is bounded
  by the stream deadline.
- `dns.ttl` re-reads only records the resolvers show below 60 s: a cached TTL only counts down, so
  a high resolver TTL is never a false finding, while a low one may be; this keeps the extra
  queries to the records in doubt (implementer under the operator's autonomy mandate, 2026-10-22).
- Single-label names are refused for domain runs. This reverses the earlier input rule pinned by
  lens's `single_label_accepted` test ("a DNS-level concern"): a single-label name has no
  public mail, web or DNS posture to check, and the OG card for `landing` and `dns` ran full
  domain runs (inventory area 10). The DNS inspector keeps single-label names (implementer under
  the operator's autonomy mandate, 2026-10-22).
- The result page keeps routing a dotless segment to its fallback instead of refusing it: the
  router's contract (`result_page_csp`) sends every non-domain single segment there, and the API
  the page would call refuses the name (implementer under the operator's autonomy mandate,
  2026-10-22).
- Review of the range (implementer under the operator's autonomy mandate, 2026-10-22): fixed on
  this branch rather than split: a failed batch at the name no longer marks the zone checks
  unmeasured once the zone's replacement answered (zone batches carry `source: "zone"`); RRSIGs are
  not re-read for TTL; a CAA climb cut off by the deadline makes `dns.caa` unmeasured. The SDD rows
  for requirements 3 to 7 landed in the phase 5 commit, not with their code; recorded, not rewritten.
- A section with every check unmeasured is labelled `incomplete` without dropping its checks: the
  grade already treats it so, and the per-check findings say why (implementer under the operator's
  autonomy mandate, 2026-10-22).

## Open decisions

None.

## Out of scope

- Operator proposals (inventory 10.19, 10.20): whether a missing HTTPS/SVCB record should fail
  `dns.https_svcb`; whether domain runs should pay the DNS module's per-target limiter keyed by
  resolver (19 per run against production's burst 53 and 160 per minute).
