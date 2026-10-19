# Spec: v2-dns-api

Status: Ready for Implementation
Created: 2026-10-19

## Goal

The DNS functions of API v1 carry what V1 prism gave its users and what V2 dropped (inventory theme
D, backend half). A query, compare or authcompare result says per record type which server answered
what, how fast, and whether the servers agree, plus a run summary. The dig-like query language works
as a curl one-liner under `/api/v1` (`GET /dns/query?q=`), as do `POST /dns/parse`, the resolver
catalogue, the record-type list, cached result replay by key and a DNS-only health check. Every new
path and component is in the OpenAPI document. Closes inventory rows 2.16, 2.19, 2.24 and 2.25 (data
part), 2.29 and 2.30 (per-server split), 2.48, 2.52, 2.53, 2.54, 7.79, 7.84, 7.85, 7.86, 7.87 and
7.88 (`netray-ops` `specs/netray/inventory/sections/02-dns.md`, `07-site-api.md`, private).

## Non-goals

- Any UI: the servers tab, view toggles, explain mode, record expansion, autocomplete, history,
  shortcuts and the `?r=` share link of the inspector page (rows 2.3, 2.4, 2.17, 2.18, 2.35–2.43).
  They are lane A's, built later against the design gate on the data this feature adds. No file under
  `crates/web/templates`, `crates/web/static` or `crates/web/islands` changes.
- The evidence blocks of the inspector page (`crates/dns/src/jobs/blocks.rs`) keep their shape.
- V1 paths: the old `dns.` API stays `410` (operator: old V1 API clients are ignored).
- `GET /api/config` (row 7.89) and the limit fields of V1 `/api/meta` (row 2.55): consumed only by
  the V1 SPA.
- The scope of the DNS checks on subdomains (inventory RESULT? flags, area 10).
- IP enrichment rendering (2.22) and inspector links (2.23).

## Context and constraints

- V1 behaviour reference: `crates/dns/src/api/{query,parse,meta,results}.rs`,
  `crates/dns/src/result_cache.rs`, and the V1 frontend's agreement logic
  (`crates/dns/frontend/src/components/ResultsTable.tsx` `lookupsAgree`, `parseSoaSerial`;
  `ServerComparison.tsx` per-server grouping and median latency).
- The API JSON already carries every server's lookup inside `batches[].lookups` (mhost's
  `Lookups`), the trace hops' `server_name`, `answer_records`, `referral_ns` and `referral_groups`,
  and each DNSSEC level's `dnskey_records`, `ds_records` and `rrsig_records`. What is missing is the
  derived comparison, the run summary and every endpoint named in the goal.
- The V1 wire (prism's `/api/*` routes and the `prism-*` contract goldens) stays byte-identical
  (CLAUDE.md: new data on V1 structs is `#[serde(skip)]` or skipped when absent). New data is V2
  only: on the collected results and as a new SSE event emitted by the v1 layer.
- Rule M: a lookup that failed (SERVFAIL, REFUSED, a referral, a timeout, a resolver error) is
  `unmeasured`, never an answer, and never counts as a divergence.
- API v1 is additive only (SDD §7.1): new paths, new components, new optional fields. Each new part
  is composed onto the finished v1 document (`api/compose.rs`).
- The `[api]` per-IP limiter covers reads; DNS jobs charge the `[api.dns]` limiter.
- SDD §7.1 and §7.3 say `/api/results/{key}` is cut and `?r=` ignored, and §8 says DNS `+check` is
  not a job. The operator's inventory decision "D DNS depth: rebuild" (`v2-parity.md`, 2026-10-19)
  supersedes both for the API; the SDD is amended in the same commit as the code.
- Local outbound DNS of the binary is blocked (Little Snitch); tests run on the offline stub
  (`netray_dns::testing`).

## Requirements

1. **Per-server answer.** Each lookup of a query, compare or authcompare job maps to one
   `DnsServerAnswer`: `server` (the name server as mhost prints it, e.g.
   `udp:8.8.8.8:53,name=Google 1`), `address` (`ip:port`), `name` (the provider label when the
   server has one), `transport` (`udp`, `tcp`, `tls`, `https`), `source` (`recursive` or
   `authoritative`, authcompare only), `outcome`, `response_code` (of an `unmeasured` answer that
   has one; `outcome` already names NXDOMAIN and NODATA), `error` (the reason of an `unmeasured` answer), `response_time_ms` (absent when the
   lookup has no response time) and `records`.
   - `outcome` is `answered` (records), `no_records` (NODATA), `nxdomain` (NXDOMAIN) or
     `unmeasured` (Rule M: any other negative answer, a referral, an error).
   - Each record is `{name, type, ttl, data}` with `data` in zone-file presentation form (as the
     evidence blocks print it), in canonical order (type, data).
2. **Record details.** A SOA record carries `soa_serial_age_days` when its serial reads as
   `YYYYMMDDnn` with a plausible date (year 1990–2100, month 1–12, day 1–31, a real calendar day):
   whole days from that date to today (UTC), as V1's `parseSoaSerial`. Any other serial has no such
   field.
3. **Agreement per record type.** For each record type of a job, a `DnsTypeComparison`
   `{record_type, agreement, answers}` holds every server answer of that type (for compare: every
   transport's; for authcompare: recursive and authoritative), sorted by source, transport, server.
   - `agreement` is `agree` when at least two answers are measured (`answered`, `no_records` or
     `nxdomain`) and all measured answers have the same outcome and the same record data (TTL
     ignored); `diverge` when two measured answers differ; `single` when exactly one is measured;
     `unmeasured` when none is.
   - Comparisons are sorted by record type name.
4. **Server summary.** For each distinct server (and source), a `DnsServerSummary`
   `{server, name, transport, source, lookups, measured, unmeasured, median_response_time_ms}`, the
   median taken over that server's measured lookups (the mean of the two middle
   values for an even count), absent when none has; sorted by source, transport, server.
5. **Run summary.** `DnsJobSummary` `{record_types, lookups, agree, diverge, single, unmeasured,
   types, servers}`: the counts over requirement 3's comparisons and the lookups behind them.
   - The collected results `DnsQueryResult`, `DnsCompareResult` and `DnsAuthCompareResult` gain
     `summary`: the `DnsJobSummary` of their batches once `done` arrived, `null` when the job ended
     before `done`.
   - With `Accept: text/event-stream`, the v1 routes `/dns/query`, `/dns/compare` and
     `/dns/authcompare` emit one `summary` event carrying the same object immediately before `done`.
   - prism's V1 routes emit no `summary` event and their payloads do not change.
6. **Trace and DNSSEC details are kept.** The v1 trace result carries per hop and server
   `server_name`, `answer_records` (the final answer), `referral_ns` and `referral_groups`; the v1
   dnssec result carries per level `dnskey_records`, `ds_records` and `rrsig_records`, each
   documented in the OpenAPI document.
7. **Query string on GET.** `GET /api/v1/dns/query?q=<query>` parses `q` with the query language
   (`netray_dns::parser::parse`, the parser of the inspector page) and runs the query job, answering
   as `POST /dns/query` does (JSON, or SSE with `Accept: text/event-stream`), charging the same
   `[api.dns]` limiter.
   - `+dnssec` adds DNSKEY and DS to the queried types (V1 `/api/query`).
   - Parser warnings (an unknown token, a truncated server group) arrive in `done.warnings`.
   - A missing or empty `q` answers `400 INVALID_INPUT`; a parse error (empty query, blocked type,
     `ALL` over the limit) answers `400 INVALID_INPUT` with the parser's message; a policy refusal
     answers as the POST route does (`400`, `403 TARGET_BLOCKED`, `429`).
8. **The other jobs on GET.** `GET /api/v1/dns/{compare,authcompare,trace,dnssec}?q=` run that job
   on the parsed query: compare and authcompare over the parsed types and servers; trace over the
   first parsed type (A when none), dnssec over the domain.
9. **Routing flags.** In `GET /dns/query`, `+trace`, `+compare`, `+auth` or `+check` answers
   `307 Temporary Redirect` with `Location: /api/v1/dns/<trace|compare|authcompare|check>?q=<q>`
   (the same `q`, percent-encoded), checked in that order; nothing is charged. `+dnssec` stays a
   query flag (requirement 7).
10. **Parse.** `POST /api/v1/dns/parse` `{input, cursor_pos?}` answers `{tokens, completions,
    query, error}` without running anything:
    - `tokens` and `completions` as V1 `/api/parse` (classified tokens with UTF-8 byte ranges,
      completions at the cursor; `@system` only when the DNS config allows system resolvers);
    - `query`, when `input` parses: `{domain, record_types, servers, transport, dnssec, warnings}`
      with servers as written by the parser (provider names, `system`, `ip:port`), else `null`;
    - `error`, the parser's message when it does not parse, else `null`.
    - The route sits behind the `[api]` per-IP limiter; a malformed body answers `400`.
11. **Resolver catalogue.** `GET /api/v1/dns/servers` answers `{providers, groups}`: each predefined
    provider `{name, configs}` with `configs` `[{protocol, description}]` as V1 `/api/servers`, and
    the parser's group aliases `{name, providers}` (`public`, `all`), under the `[api]` per-IP
    limiter.
12. **Record types.** `GET /api/v1/dns/record-types` answers `{record_types}`, each `{name}`, every
    type mhost knows except ANY, AXFR, IXFR, OPT, ZERO and NULL, as V1 `/api/record-types`, under the
    `[api]` per-IP limiter.
13. **Cache key.** Every v1 DNS job route (query, compare, authcompare, trace, dnssec, in both forms,
    POST and GET, and the check of requirement 15) stores its collected result when it finishes with
    `done` (the check: when it answers) under a fresh key of 12 lowercase hex characters, all random
    bits (a UUID v4's first 12), and returns it in `done.cache_key` (the check: `cache_key`). The cache holds at most
    1 000 results for one hour each (V1's bounds). A job that ended without `done` stores nothing and
    carries no key. `DnsDnssecDone` gains an optional `cache_key`, absent on prism's wire.
14. **Replay.** `GET /api/v1/dns/results/{key}` answers `{key, mode, query, result}`: `mode` one of
    `query`, `compare`, `authcompare`, `trace`, `dnssec`, `check`; `query` the `q` of a GET or the
    domain of a POST; `result` the stored collected result (with its `summary`). A key that is not 12
    lowercase hex characters answers `400 INVALID_INPUT`, an unknown or expired one `404 NOT_FOUND`. Under the
    `[api]` per-IP limiter.
15. **DNS health check.** `POST /api/v1/dns/check` `{domain}` and `GET /api/v1/dns/check?q=` (the
    domain is `q`'s first token) run the DNS module alone on that name, live and not stored, and
    answer `{protocol, outcome, reason, checks, blocks, cache_key}` as one section of a run
    (`RunSectionEvent`'s fields), JSON whatever the `Accept` header.
    - The name is the POST body's whole `domain`, or `q`'s first token. It must pass lens's domain
      rule (`netray_common::domain::validate`) and hold only ASCII letters, digits, `-`, `_` and
      `.`; an IP address (a trailing dot included), an empty name, a name with whitespace or any
      other character answers `400 INVALID_INPUT`. The check never runs on a name other than the
      one asked.
    - It charges the `[api.dns]` limiter a flat 16 per caller and globally (as trace; V1 `/api/check`
      charged per step) and runs under a 30 s deadline.
    - With no DNS module built it answers `503 UNAVAILABLE`.
16. **OpenAPI.** Every new path, method and component is in `/api/v1/openapi.json`, composed onto
    the finished v1 document; the committed contract `api-v1.openapi.json` grows compatibly only.
    Every 2xx answer of the new routes validates against its documented schema.
17. **SDD.** `specs/sdd/v2.md` §7.1's endpoint table lists the new DNS endpoints and the `summary`
    event, §7.3 no longer says `/api/results/{key}` is cut (the v1 replay exists; `?r=` on the
    inspector page stays lane A's), and §8's "+check is not a job" names the v1 check endpoint.

## Phase 1 — Per-server comparison and run summary

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6

`crates/dns/src/jobs/summary.rs` (new): pure functions from batches to `DnsJobSummary`; the
collected results gain `summary`; `DnsJobEvent` gains the summary hook the v1 layer uses to emit the
`summary` event; `crates/web/src/api/dns.rs` emits it before `done`. The offline stub gains a name
whose servers diverge and a SOA answer.

### Test Scenarios

GIVEN the stub and `POST /dns/query` for `example.com` A as JSON WHEN it finishes THEN `summary.types` has one A comparison with agreement `agree`, one answer per configured server, each `answered` with record `192.0.2.10`, a transport and a response time.
GIVEN a stub name whose servers return different A records WHEN queried THEN the A comparison is `diverge` and each answer keeps its own records.
GIVEN a type that SERVFAILs on every server (`dnssec-failed.org` A) WHEN queried THEN every answer is `unmeasured` with response code `SERVFAIL` and the comparison is `unmeasured`.
GIVEN `partial.example.net` A and AAAA WHEN queried THEN A is `agree` and AAAA is `unmeasured`, and `summary.agree` is 1 and `summary.unmeasured` is 1.
GIVEN a type every server answers NODATA WHEN queried THEN each answer is `no_records` and the comparison is `agree`.
GIVEN one measured answer and one SERVFAIL for the same type WHEN compared THEN agreement is `single`, not `diverge`.
GIVEN two answers with the same records and different TTLs WHEN compared THEN agreement is `agree`.
GIVEN a server with response times 10, 30 and 20 ms WHEN summarised THEN its median is 20; GIVEN 10 and 30 THEN 20; GIVEN only errors THEN no median.
GIVEN serial 2026101501 and today 2026-10-19 WHEN aged THEN `soa_serial_age_days` is 4; GIVEN serial 12345 or 2026139901 THEN there is no age.
GIVEN a stub SOA answer WHEN queried THEN the SOA record carries its `data` in presentation form and `soa_serial_age_days`.
GIVEN `POST /dns/compare` on the stub WHEN it finishes THEN each type's comparison holds answers of all four transports and the transports agree.
GIVEN `POST /dns/authcompare` on the stub WHEN it finishes THEN each type's comparison holds recursive and authoritative answers with source set, and they agree.
GIVEN `POST /dns/query` with `Accept: text/event-stream` WHEN streamed THEN one `summary` event precedes `done` and equals the JSON form's `summary`, valid against `DnsJobSummary`.
GIVEN prism's V1 `/api/query` on the same stub WHEN streamed THEN it has no `summary` event and its frames equal v1's frames without `summary` and `cache_key`.
GIVEN `POST /dns/trace` on the stub WHEN it finishes THEN the final hop's server results carry `answer_records` with `192.0.2.10` and a `server_name`; GIVEN `POST /dns/dnssec` THEN each level has `dnskey_records`, `ds_records` and `rrsig_records`.

## Phase 2 — Query language on GET

**Depends on:** Phase 1
**Requirements:** 7, 8, 9

GET handlers beside the POST ones in `crates/web/src/api/dns.rs`, sharing their start and respond
paths.

### Test Scenarios

GIVEN `GET /dns/query?q=example.com MX @google` WHEN answered THEN status 200, the batches hold MX only, every lookup is a Google server, and the body validates against `DnsQueryResult`.
GIVEN `GET /dns/query?q=example.com` WHEN answered THEN the batches are A, AAAA, CNAME and MX.
GIVEN `GET /dns/query?q=example.com A +dnssec` WHEN answered THEN the batches are A, DNSKEY and DS and `done.dnssec` is true.
GIVEN `q=example.com A bogus` WHEN answered THEN status 200 and `done.warnings` names the unrecognized token.
GIVEN no `q`, or `q=` empty WHEN answered THEN `400 INVALID_INPUT`.
GIVEN `q=example.com ANY` WHEN answered THEN `400 INVALID_INPUT` naming the blocked type.
GIVEN `q=example.com A @192.168.1.1` WHEN answered THEN the POST route's refusal for that server.
GIVEN `Accept: text/event-stream` on `GET /dns/query` WHEN answered THEN named events `batch`, `summary`, `done`.
GIVEN `GET /dns/compare?q=example.com A` WHEN answered THEN a `DnsCompareResult` with four transports; GIVEN `/dns/authcompare?q=example.com A` THEN a `DnsAuthCompareResult`.
GIVEN `GET /dns/trace?q=example.com MX` WHEN answered THEN a `DnsTraceResult`; GIVEN `/dns/dnssec?q=example.com` THEN a `DnsDnssecResult`.
GIVEN `GET /dns/query?q=example.com +trace` WHEN answered THEN `307` to `/api/v1/dns/trace?q=example.com%20%2Btrace`; `+compare`, `+auth`, `+check` redirect to compare, authcompare and check.
GIVEN a caller over the `[api.dns]` budget WHEN `GET /dns/query?q=example.com` THEN `429` with `Retry-After`.

## Phase 3 — Parse, servers and record types

**Depends on:** none
**Requirements:** 10, 11, 12

`crates/dns/src/api/parse.rs` and `meta.rs` expose their logic as functions the v1 routes call.

### Test Scenarios

GIVEN `{"input": "example.com MX @cl"}` WHEN parsed THEN tokens are domain, record_type and server_partial with byte ranges, completions include `@cloudflare`, and `query.servers` is empty with a warning naming `@cl`.
GIVEN `{"input": "example.com A @google +tcp"}` WHEN parsed THEN `query` is domain `example.com`, types `["A"]`, servers `["google"]`, transport `tcp`, and `error` is null.
GIVEN `{"input": "example.com ANY"}` WHEN parsed THEN `query` is null and `error` names the blocked type.
GIVEN `{"input": "", "cursor_pos": 0}` WHEN parsed THEN tokens are empty and `error` is the empty-query message.
GIVEN a body that is not JSON WHEN posted THEN `400`.
GIVEN `allow_system_resolvers = false` WHEN `{"input": "example.com @"}` is parsed THEN completions do not offer `@system`; GIVEN it true THEN they do.
GIVEN `GET /dns/servers` WHEN answered THEN providers include `cloudflare`, `google`, `quad9` and `mullvad`, each with udp, tcp, tls and https configs, and groups `public` and `all` list their providers.
GIVEN `GET /dns/record-types` WHEN answered THEN the list holds A, MX, DNSKEY, TLSA and SVCB and holds none of ANY, AXFR, IXFR, OPT, ZERO, NULL.
GIVEN each of the three routes WHEN answered THEN the body validates against its documented schema.

## Phase 4 — Result cache and replay

**Depends on:** Phase 2
**Requirements:** 13, 14

A typed cache of collected results (moka, 1 000 entries, one hour) held by `ApiState`; the v1 respond
path stores at `done` in both forms.

### Test Scenarios

GIVEN `POST /dns/query` as JSON WHEN it finishes THEN `done.cache_key` is 12 lowercase hex characters, and `GET /dns/results/<key>` answers mode `query`, query `example.com` and a result equal to the original body.
GIVEN `GET /dns/query?q=example.com MX` streamed WHEN it finishes THEN the `done` event's `cache_key` replays with query `example.com MX` and the `summary` included.
GIVEN compare, authcompare, trace and dnssec WHEN each finishes THEN each `done` carries a key that replays with its mode.
GIVEN two runs WHEN finished THEN their keys differ.
GIVEN `GET /dns/results/XYZ` or a 13-character key WHEN answered THEN `400 INVALID_INPUT`; GIVEN an unknown 12-hex key THEN `404 NOT_FOUND`.
GIVEN prism's V1 dnssec route WHEN streamed THEN its `done` has no `cache_key`.
GIVEN a replay WHEN answered THEN it validates against `DnsCachedResult`'s documented schema.

## Phase 5 — DNS health check

**Depends on:** Phase 4
**Requirements:** 15

### Test Scenarios

GIVEN the golden DNS module and `POST /dns/check {"domain": "example.com"}` WHEN answered THEN status 200, protocol `dns`, outcome `measured`, the ten `dns.*` checks with status and findings, the evidence blocks, and a `cache_key` that replays with mode `check`.
GIVEN `GET /dns/check?q=example.com` WHEN answered THEN the same shape.
GIVEN `{"domain": "192.0.2.1"}` or `{"domain": ""}` WHEN posted THEN `400 INVALID_INPUT`.
GIVEN no DNS module WHEN posted THEN `503 UNAVAILABLE`.
GIVEN a caller over the `[api.dns]` budget WHEN posted THEN `429`.
GIVEN `Accept: text/event-stream` WHEN posted THEN the answer is still JSON.

## Phase 6 — Contract, SDD and behavioural check

**Depends on:** Phases 1–5
**Requirements:** 16, 17

The committed OpenAPI contract is regenerated (compatible growth only), the SDD amended, and the
running binary probed: `curl` against a local `netray lens` for `servers`, `record-types`, `parse`
and a query (the query's DNS is blocked locally; its refusal or timeout shape is recorded, not
invented).

### Test Scenarios

GIVEN the generated document WHEN compared with the committed contract THEN the only differences are added paths, methods, components and optional properties.
GIVEN the document WHEN read THEN it has `/api/v1/dns/query` get and post, `/dns/{compare,authcompare,trace,dnssec}` get and post, `/dns/check` get and post, `/dns/parse` post, `/dns/servers`, `/dns/record-types` and `/dns/results/{key}` get, and the components `DnsJobSummary`, `DnsTypeComparison`, `DnsServerAnswer`, `DnsServerSummary`.
GIVEN `specs/sdd/v2.md` WHEN read THEN §7.1 lists the new endpoints and §7.3 and §8 no longer say the replay is cut and `+check` has no endpoint.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).
- The per-server comparison is derived in netray-dns from the batches the job already collects, not
  sent per batch: the `batch` event stays prism's payload, so V1 and v1 batch frames remain equal;
  the comparison arrives once, in `summary` (implementer under the operator's autonomy mandate,
  2026-10-19).
- Rule M decides agreement: an unmeasured answer is neither agreement nor divergence, and one
  measured answer is `single`. V1's `lookupsAgree` counted an error as a divergence (implementer under
  the operator's autonomy mandate, 2026-10-19).
- GET forms live on each job's own path, with routing flags in `GET /dns/query` as a 307: one
  response schema per path, and `curl -L` follows the flag (implementer under the operator's autonomy
  mandate, 2026-10-19).
- SSE stays chosen by `Accept`, as on every v1 route; V1's `?stream=` parameter is not ported
  (implementer under the operator's autonomy mandate, 2026-10-19).
- Cache keys use random bits: V1's key was the first 12 hex characters of a UUID v7, its timestamp,
  so two results in the same millisecond collided and keys were guessable (implementer under the
  operator's autonomy mandate, 2026-10-19).
- The check endpoint answers one section as JSON only: one section has nothing to stream
  progressively (implementer under the operator's autonomy mandate, 2026-10-19).
- The SDD conflict (§7.1/§7.3 cut the replay, §8 no `+check` job) is resolved by the operator's
  inventory decision "D DNS depth: rebuild"; the SDD is amended with the code, not halted on
  (implementer under the operator's autonomy mandate, 2026-10-19).

- Phase 1 amendment: `response_code` is set on unmeasured answers only and the server median is
  taken over measured lookups only, as the tests pin them; a SERVFAIL's quick answer would
  otherwise pull a server's median down (implementer under the operator's autonomy mandate,
  2026-10-19).

- Phase 1 review: a server whose query never became a lookup (an authoritative timeout, a
  resolver error, a provider skipped by an open circuit breaker) reaches the caller in `errors`,
  as before, and is absent from `summary`: the summary covers lookups only. An authoritative
  referral is unmeasured; authoritative failures carry the rcode mnemonic; across sources, types
  the two record printers write differently (DNSSEC types) compare by record count. The `summary`
  event is a variant of each batch job's event schema (implementer under the operator's autonomy
  mandate, 2026-10-19).

- Final review: the check's name rule replaces the query parser (which validates no syntax: an
  over-long name ran, `example.com other` checked `example.com`); internationalised names are
  refused until the A-label is given, as lens's rule leaves IDNA out. Keys come from a UUID v4: a
  v7's last 48 bits hold 16 counter bits shared by keys of one millisecond. `/dns/servers`,
  `/dns/record-types` and `/dns/parse` answer without a DNS part (parse then offers no `@system`).
  The SDD amendment landed in Phase 6's commit, not with each phase's code: a bisect onto phases
  2 to 5 reads the older §7 (implementer under the operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

Rendering any of this on the inspector page, the `?r=` share link of inspector runs (needs the page
runs to store results: lane A can reach the cache through `ApiState`), tracing every type of a
multi-type `+trace`, and the V1 `/api/config` and `/api/meta` limit fields.
