# Spec: v2-input-edges

Status: Ready for Implementation
Created: 2026-10-19

## Goal

Two input edge cases left open by v2-input-limits (PR #40) are closed. The shared domain rule
(`netray_common::domain::validate`) maps its input with UTS #46 (IDNA) before it validates, so a
name is judged as what it means: `１.１.１.１` is the address `1.1.1.1` and is refused, a name with
whitespace inside is refused, and full-width letters are accepted as their ASCII form. And a DNS or
TLS request whose cost exceeds a limiter's burst, which no wait can ever admit, is refused up front
as invalid input naming the cost and the limit, instead of a `429` with `Retry-After: 60` that no
retry can pass; `429` stays for requests that pass after waiting.

## Non-goals

- The TLS and HTTP inspectors' own target parsing: they take addresses on purpose.
- The DNS query language's domain token (`netray_dns::parser`) and the DNS check route's own
  ASCII-only rule (`crates/web/src/api/dns.rs` `check_domain`, which already refuses `１.１.１.１`).
- Showing an IDN as its Unicode form on pages: the UI half (lane A) may render the stored A-label
  as a U-label; this feature stores and answers the A-label.
- Limiter changes on the domain-run path (lens admission, badge and OG), the module runs'
  per-target limiters (`crates/{dns,tls,http}/src/module.rs`) and the V1 `netray ip` service.
- Any file under `crates/web/templates`, `crates/web/static` or `crates/web/islands`.

## Context and constraints

- `validate` today trims, refuses addresses (bare, one trailing dot, bracketed) and a numeric last
  label, lowercases, refuses wildcards, label and name lengths. It checks no characters, so
  `１.１.１.１` (full-width digits, U+FF11) and `1.1.1.1 .` pass, and the HTTP module's URL parser
  later maps the first to IPv4 `1.1.1.1`.
- `idna` 1.1.0 is already in the tree (via `url` and hickory) and allowed by `cargo deny`; using it
  from `netray-common` adds a direct dependency, no new crate.
  `idna::uts46::Uts46::to_ascii` with `AsciiDenyList::URL` (the WHATWG forbidden domain code points
  plus C0 controls and space), `Hyphens::Allow` and `DnsLength::Ignore` is the URL Standard's
  domain-to-ASCII: it lowercases, maps full-width forms and the ideographic full stop, applies NFC,
  punycodes non-ASCII labels, checks `xn--` labels, and refuses space, `%`, `/`, `:`, `@` and the
  other forbidden code points. The deny list refuses `[`, so address checks run on the raw input
  first, as now.
- Callers of `validate` (as #40 listed them): lens's `validate_domain` (API v1 `DomainRuns`, result
  page, badge and OG routes, V1 `/api/check`), the evidence pages (`crates/web/src/pages/evidence.rs`),
  `registrable_domain` (`crates/web/src/pages/inspect.rs`) and the DNS check route. The result page
  answers a non-canonical name with a 301 to the validated form.
- lens's unit test `punycode_accepted` pins that an A-label input is answered unchanged.
- Limiters are governor GCRA buckets. `check_key_n` with a cost above the burst returns
  `InsufficientCapacity`, which `netray_common::rate_limit::check_keyed_cost` and
  `check_direct_cost_scoped` turn into a rejection with `retry_after_secs: 60`. A limiter checked
  earlier in the same request has already spent its tokens by then (CLAUDE.md: governor's `check`
  spends a token whatever follows).
- DNS (`crates/dns/src/security/rate_limit.rs`, `check_query_cost`): total cost against the per-IP
  and global bursts, per-target cost (the record types) against the per-target burst. Query cost is
  types x servers, compare types x servers x available transports, authcompare
  types x servers x 2 + 16, trace and dnssec 16. API v1 shares `netray_dns::security::RateLimitState`
  (`[api.dns]`, defaults per-IP burst 64, per-target 20, global 50); prism's V1 routes use `[limits]`
  with the same defaults. A compare over three servers with three types plus `+dnssec` costs
  5 x 3 x 4 = 60 against the global burst 50.
- DNS refusals reach API v1 through `DnsRefusal`: anything mapped to `DnsRefusal::InvalidInput` is
  `400 INVALID_INPUT`, and the inspect page's DNS gate answers it as one notice. prism's V1 limit
  refusals (`TOO_MANY_RECORD_TYPES`, `TOO_MANY_SERVERS`) are `422`.
- TLS (`crates/tls/src/security/rate_limit.rs`, `check_cost`): cost ports x inspected addresses
  against the per-IP and per-target bursts. The inspector (`crates/tls/src/inspector.rs`) caps the
  addresses to `remaining_budget` (the per-IP burst) when the full cost is refused, and answers
  `RateLimited { retry_after_secs: 60 }` when not even one address fits. tlsight's V1 limit refusal
  `TOO_MANY_PORTS` is `422`; API v1 maps `TlsRefusal::InvalidInput` to `400 INVALID_INPUT`. The
  inspect page's TLS admission (`crates/web/src/api/jobs.rs` `admit_tls`) charges the ports against
  `[api.tls]` and answers any refusal as `429`.
- Checked and unchanged: HTTP and email charge 1 per request against bursts validated non-zero;
  API v1 ip charges 1 (diff 2, `per_ip_burst` validated at least 2) and the batch's address count,
  bounded by the validation `batch_addresses_burst >= batch_max_size`; lens's limiters charge 1.

## Requirements

1. **IDNA mapping.** `validate` maps the trimmed input with UTS #46 (`to_ascii`, URL deny list,
   hyphens allowed, DNS length ignored) after its existing address checks on the raw input and
   before every other check, and validates and returns the mapped ASCII form. A mapping error
   refuses the input with `not a valid domain name: <trimmed input>`.
2. **Mapped addresses.** The address and numeric-last-label checks run again on the mapped form:
   `１.１.１.１`, `１.１.１.１.` and `０x７f.1` are refused with the existing messages, which name the
   trimmed input.
3. **Whitespace and forbidden characters inside.** `1.1.1.1 .`, `exa mple.com`, `example.com/x`,
   `ex%61mple.com` and a name with a tab inside are refused (requirement 1's message).
4. **One domain, one form.** Full-width letters, the ideographic full stop, uppercase and
   Unicode labels map to one ASCII form: `ＥＸＡＭＰＬＥ.com`, `example。com` and `Example.COM.`
   answer `example.com`; `bücher.de` and `xn--bcher-kva.de` both answer `xn--bcher-kva.de`.
   `123.example.com`, `1password.com`, `localhost`, `_dmarc.example.com` and
   `xn--nxasmq6b.xn--jxalpdlp` answer as before.
5. **Every caller.** With lens's real rule: `POST /api/v1/domains/１.１.１.１/runs` answers
   `400 DOMAIN_INVALID` and records no run; `GET /ＥＸＡＭＰＬＥ.com` answers a 301 to `/example.com`;
   the evidence page `/１.１.１.１/dns` answers 404.
6. **DNS cost beyond a burst.** `check_query_cost` compares, before the stream check and before any
   limiter is charged, the total cost with the per-IP and global bursts and the per-target cost with
   the per-target burst, in that order. The first one exceeded refuses the request with a new
   `ApiError::CostExceedsLimit { cost, limit }` whose message names both numbers
   (`request cost <cost> exceeds the limit of <limit> per request`) and charges no limiter.
7. **DNS answers.** prism's V1 routes answer `CostExceedsLimit` with `422` and code
   `COST_EXCEEDS_LIMIT`, without `Retry-After`. API v1's DNS routes (`POST` and `GET` query,
   compare, authcompare) answer `400 INVALID_INPUT` with the message, without `Retry-After`.
8. **TLS cost beyond a burst.** `check_cost` compares the cost with the per-IP and per-target bursts
   before charging either and refuses with a new `AppError::CostExceedsLimit { cost, limit }`
   (same message shape; tlsight V1 `422 COST_EXCEEDS_LIMIT` without `Retry-After`; API v1
   `400 INVALID_INPUT`). `remaining_budget` answers the smaller of the two bursts, and the inspector
   answers `CostExceedsLimit { cost: ports, limit: budget }` where it answered `RateLimited` with 60
   s because not one address fits. The inspect page's TLS admission lets such a request through
   uncharged and its job start answers the message as the notice.
9. **429 stays.** A request within every burst that finds too few tokens left still answers `429`
   (`RATE_LIMITED` with `Retry-After`) on prism V1, tlsight V1 and API v1.

## Phases

## Phase 1 — IDNA mapping in the domain rule

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5

`crates/common/src/domain.rs` maps with `idna` between the raw address checks and the rest;
`idna` becomes a workspace dependency of `netray-common`. Tests in `crates/common/tests/it/` for
the rule and in `crates/lens/tests/it/` for the routes, one for the evidence page in
`crates/web/tests/it/`.

### Test Scenarios

GIVEN `１.１.１.１` or `１.１.１.１.` WHEN validated THEN refused with a message starting `IP addresses are not accepted as domain input`.
GIVEN `０x７f.1` WHEN validated THEN refused with a message starting `the last label is a number`.
GIVEN `1.1.1.1 .`, `exa mple.com`, `example.com/x`, `ex%61mple.com` or `exa\tmple.com` WHEN validated THEN refused with a message starting `not a valid domain name`.
GIVEN `ＥＸＡＭＰＬＥ.com`, `example。com` or `Example.COM.` WHEN validated THEN `example.com`.
GIVEN `bücher.de` or `xn--bcher-kva.de` WHEN validated THEN `xn--bcher-kva.de`.
GIVEN `123.example.com`, `1password.com`, `localhost`, `_dmarc.example.com` or `xn--nxasmq6b.xn--jxalpdlp` WHEN validated THEN answered unchanged.
GIVEN lens's app WHEN `POST /api/v1/domains/１.１.１.１/runs` THEN `400` with `error.code` `DOMAIN_INVALID` and no run recorded.
GIVEN lens's app WHEN `GET /ＥＸＡＭＰＬＥ.com` THEN `301` with `location` `/example.com`.
GIVEN the evidence routes WHEN `GET /１.１.１.１/dns` THEN `404`.

## Phase 2 — Refuse a cost no wait can admit

**Depends on:** none
**Requirements:** 6, 7, 8, 9

`crates/dns/src/security/rate_limit.rs` and `crates/tls/src/security/rate_limit.rs` keep their
bursts and check them first; the new variants in `crates/dns/src/error.rs` and
`crates/tls/src/error.rs` map to `DnsRefusal::InvalidInput` and `TlsRefusal::InvalidInput`;
`crates/tls/src/inspector.rs` and `crates/web/src/api/jobs.rs` (TLS admission and start) follow.
Tests in the `jobs` testing target and `jobs_prism` of netray-dns, netray-tls's tests and
`crates/web/tests/it/` for API v1.

### Test Scenarios

GIVEN a DNS limiter with global burst 20 WHEN `DnsJobs::query` runs eight types plus `dnssec` over cloudflare, google and quad9 (cost 30) THEN `Err(CostExceedsLimit { cost: 30, limit: 20 })` and the limiter still admits a cost of 20 from the same client.
GIVEN a DNS limiter with per-target burst 5 WHEN `DnsJobs::compare` runs A, AAAA, MX, TXT plus `dnssec` THEN `Err(CostExceedsLimit { cost: 6, limit: 5 })`.
GIVEN prism's V1 app with default limits WHEN `POST /api/compare` with A, AAAA, MX, `dnssec` true, servers cloudflare, google and quad9 THEN `422` with code `COST_EXCEEDS_LIMIT`, a message naming 60 and 50, and no `retry-after` header.
GIVEN API v1 with default `[api.dns]` WHEN `POST /dns/compare` with that body THEN `400 INVALID_INPUT` naming 60 and 50, no `retry-after`.
GIVEN API v1 with `[api.dns] global_burst = 20` WHEN `POST /dns/query` with eight types, `dnssec` true and three servers, or `GET /dns/query?q=` with the same query in the query language THEN `400 INVALID_INPUT` naming 30 and 20, no `retry-after`.
GIVEN API v1 with `[api.dns] global_burst = 20` WHEN two `POST /dns/query` of cost 18 from one client THEN the second answers `429 RATE_LIMITED` with `retry-after`.
GIVEN a TLS limiter with per-IP burst 2 WHEN `check_cost` with cost 3 THEN `Err(CostExceedsLimit { cost: 3, limit: 2 })` and the per-IP and per-target limiters still admit a cost of 2.
GIVEN a TLS limiter with per-IP burst 10 and per-target burst 4 WHEN `remaining_budget` THEN 4.
GIVEN `AppError::CostExceedsLimit` WHEN rendered by tlsight THEN `422 COST_EXCEEDS_LIMIT` without `retry-after`; WHEN mapped to `TlsRefusal` THEN `InvalidInput`.
GIVEN API v1 with `[api.tls] per_ip_burst = 2` WHEN `POST /tls/inspect` with three ports THEN `400 INVALID_INPUT` naming 3 and 2, no `retry-after`.
GIVEN the inspect page with `[api.tls] per_ip_burst = 2` WHEN the TLS job runs `example.com:443,8443,993` THEN no `429`; the answer carries the cost message as its notice.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).
- The normal form is the ASCII (A-label) form, and it is what lens stores, links and displays: an
  IDN typed as Unicode and as `xn--` is one domain with one history, every module (TLS's hostname
  rule is ASCII-only) gets a name it can use, and the 301 `location` is ASCII. An existing history
  stored under a Unicode name stays reachable only under its A-label from now on; showing the
  U-label is the UI half's choice (implementer under the operator's autonomy mandate, 2026-10-19).
- The URL deny list, not STD3: STD3 refuses `_`, which `_dmarc`-style names carry; the URL list
  refuses exactly what a URL host cannot hold, whitespace included (implementer under the
  operator's autonomy mandate, 2026-10-19).
- The cost check compares with the configured bursts, kept beside the limiters, before any charge,
  rather than reading governor's `InsufficientCapacity`: by then an earlier limiter of the same
  request has spent its tokens (implementer under the operator's autonomy mandate, 2026-10-19).
- prism and tlsight V1 get a new code `COST_EXCEEDS_LIMIT` with `422`, the status of their other
  limit refusals; no existing code of theirs names a cost. API v1 uses its existing
  `INVALID_INPUT` (400), so its code list does not grow (implementer under the operator's autonomy
  mandate, 2026-10-19).
- A `429` that could never pass becoming a `400` is a fix, not a change of meaning under API v1's
  additive rule: no client could have relied on the retry succeeding (implementer under the
  operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

Rendering IDNs as Unicode on pages, the DNS query language's domain token, and the domain-run and
module-run limiters.
