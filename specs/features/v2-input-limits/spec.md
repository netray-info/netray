# Spec: v2-input-limits

Status: Ready for Implementation
Created: 2026-10-19

## Goal

Two input limits hold that today leak. The shared domain rule (`netray_common::domain::validate`)
refuses every name that reads as an IP address, a trailing dot or brackets included, so no domain
run, result page, badge, card, evidence page or API read starts on an address. And a DNS query or
compare with `+dnssec` counts the DNSKEY and DS types it adds before the record-type limit and the
cost check, so ten types plus `+dnssec` are refused instead of running twelve. Both bugs predate V2
and were found during v2-dns-api.

## Non-goals

- The TLS and HTTP inspectors' own target parsing: they take addresses on purpose.
- A character-set rule for domain names (the shared rule checks no characters today; the DNS check
  route adds its own). Only the address forms change.
- The ip inspector's cached hostname for a `dns=false` request (`crates/ip/src/routes.rs`): PR #37
  (v2-ip-cli) is open and owns it.
- Any file under `crates/web/templates`, `crates/web/static` or `crates/web/islands`.

## Context and constraints

- Every domain caller funnels through `netray_common::domain::validate`: lens's `validate_domain`
  (API v1 `DomainRuns`, result page, badge and OG routes, V1 `/api/check`), the evidence pages
  (`crates/web/src/pages/evidence.rs`), `registrable_domain` (`crates/web/src/pages/inspect.rs`)
  and the DNS check route (`crates/web/src/api/dns.rs`, which already refuses a dotted address).
  One fix in the shared rule reaches all of them.
- Today `validate("192.0.2.1.")` strips the dot after the address test and answers `192.0.2.1`:
  `POST /api/v1/domains/192.0.2.1./runs` runs lens on an address, and the result page redirects
  `/192.0.2.1.` with a 301 before refusing.
- Refusals keep their codes: `DOMAIN_INVALID` (lens, API v1 domain routes), the result page's
  refusal page (400), the evidence pages' 404, `INVALID_INPUT` on `/api/v1/dns/check`.
- `crates/dns/src/jobs/mod.rs`: `query_parsed` adds DNSKEY/DS after `QueryPolicy::validate`
  (which holds `max_record_types`, default 10) but before the cost; `compare_parsed` adds them after
  both. Every DNS query path (prism's V1 `/api/query` GET and POST, API v1 `POST /dns/query`,
  `GET /dns/query?q=`, compare, the inspector page jobs) calls these two functions.
- The refusal stays `ApiError::TooManyRecordTypes { requested, max }` (prism `TOO_MANY_RECORD_TYPES`,
  API v1 its existing mapping).
- Tests run offline on the DNS stub (`netray_dns::testing`); integration tests are modules of each
  crate's `tests/it` binary, or `required-features = ["testing"]` targets.

## Requirements

1. **Dotted and bracketed addresses.** `validate` refuses an input that parses as an IP address
   after removing one trailing dot, or after removing surrounding square brackets: `192.0.2.1.`,
   `2001:db8::1.`, `::1.`, `[2001:db8::1]`, `[::1]`. The message is the existing one,
   `IP addresses are not accepted as domain input: <trimmed input>`.
2. **Numeric last label.** `validate` refuses a name whose last label (after removing one trailing
   dot) is a number in the WHATWG URL sense ("ends in a number"): all ASCII digits, or `0x`/`0X`
   followed by only hex digits. Such a name is read as an IPv4 address by URL parsers and
   `inet_aton` (`1.2.3`, `3221225985`, `0x7f.1`, `192.0.2.1.` written as `192.0.2.01`). Message:
   `the last label is a number, so the name reads as an IP address: <trimmed input>`. Names with
   digits elsewhere stay valid: `123.example.com`, `1password.com`, `example.com`, `localhost`,
   `a1`, `x.0xg`.
3. **Every caller refuses.** With lens's real rule: `POST /api/v1/domains/192.0.2.1./runs` and
   `GET /api/v1/domains/192.0.2.1.` answer `400 DOMAIN_INVALID`; `GET /192.0.2.1.` answers the 400
   refusal page, not a 301; `/badge/192.0.2.1..svg` answers `400 DOMAIN_INVALID`; the evidence page
   `/192.0.2.1./dns` answers 404.
4. **DNSSEC types count.** With `+dnssec` (query language) or `"dnssec": true` (POST body), DNSKEY
   and DS, each when not already requested, are added before `QueryPolicy::validate` and before the
   cost is computed, in both `query_parsed` and `compare_parsed`. Ten types plus `+dnssec` are
   refused with `TooManyRecordTypes { requested: 12, max: 10 }` on prism's V1 route, API v1
   `POST /dns/query`, `GET /dns/query?q=` and compare alike.
5. **No double count.** Eight types plus `+dnssec` run (ten types); ten types that already include
   DNSKEY and DS plus `+dnssec` run (still ten).

## Phases

## Phase 1 — Address forms in the domain rule

**Depends on:** none
**Requirements:** 1, 2, 3

`crates/common/src/domain.rs` tests the address forms before it strips the dot; tests in
`crates/common/tests/it/` for the rule and in `crates/lens/tests/it/` for the routes, plus one
for the evidence page in `crates/web/tests/it/`.

### Test Scenarios

GIVEN `192.0.2.1.`, `2001:db8::1.`, `::1.`, `[2001:db8::1]` or `[::1]` WHEN validated THEN refused with a message starting `IP addresses are not accepted as domain input`.
GIVEN `1.2.3`, `3221225985`, `0x7f.1`, `example.0x1F`, `192.0.2.01` or `example.123.` WHEN validated THEN refused with a message starting `the last label is a number`.
GIVEN `123.example.com`, `1password.com`, `Example.COM.`, `localhost`, `a1` or `x.0xg` WHEN validated THEN accepted and normalised as before.
GIVEN lens's app WHEN `POST /api/v1/domains/192.0.2.1./runs` THEN `400` with `error.code` `DOMAIN_INVALID` and no run recorded.
GIVEN lens's app WHEN `GET /api/v1/domains/192.0.2.1.` THEN `400 DOMAIN_INVALID`.
GIVEN lens's app WHEN `GET /192.0.2.1.` THEN `400` with the refusal page, no `location` header.
GIVEN lens's app WHEN `GET /badge/192.0.2.1..svg` THEN `400 DOMAIN_INVALID`.
GIVEN the evidence routes WHEN `GET /192.0.2.1./dns` THEN `404`.

## Phase 2 — DNSSEC types inside the limit

**Depends on:** none
**Requirements:** 4, 5

`crates/dns/src/jobs/mod.rs` adds the DNSSEC types in one helper called first in `query_parsed`
and `compare_parsed`; tests in a `testing`-feature target of netray-dns and in
`crates/web/tests/it/` for the API v1 routes.

### Test Scenarios

GIVEN ten record types and `dnssec: true` WHEN `DnsJobs::query` runs THEN `Err(TooManyRecordTypes { requested: 12, max: 10 })`.
GIVEN ten record types and `dnssec: true` WHEN `DnsJobs::compare` runs THEN the same refusal.
GIVEN `example.com A AAAA MX TXT NS SOA CAA CNAME PTR SRV +dnssec` WHEN `query_parsed` runs THEN the same refusal.
GIVEN prism's V1 `GET /api/query?q=` with ten types and `+dnssec` WHEN answered THEN `400` with code `TOO_MANY_RECORD_TYPES`.
GIVEN API v1 `POST /dns/query` with ten types and `dnssec: true`, or `GET /dns/query?q=` with ten types and `+dnssec` WHEN answered THEN `400` with the code API v1 gives an eleventh type today.
GIVEN eight types and `dnssec: true` WHEN `DnsJobs::query` runs THEN the job starts and answers ten batches.
GIVEN ten types including DNSKEY and DS and `dnssec: true` WHEN `DnsJobs::query` runs THEN the job starts.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).
- The numeric-last-label rule follows WHATWG's "ends in a number" (digits, or `0x` hex) rather than
  only the dotted forms: `1.2.3` and `3221225985` reach the same address through every URL parser and
  `inet_aton`, and no public TLD is numeric (implementer under the operator's autonomy mandate,
  2026-10-19).
- The ip cache-key bug is not fixed here: PR #37 (v2-ip-cli) is open and its agent owns it
  (implementer under the operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

A character-set rule for domain names, IDNA handling (lens's rule leaves it out), and the ip
inspector's cache (PR #37).
