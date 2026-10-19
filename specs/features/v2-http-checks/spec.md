# Spec: v2-http-checks

Status: Done
Created: 2026-10-19
Finished: 2026-10-19

## Goal

The backend half of inventory theme F (HTTP analysis) holds: the HTTP evidence carries the parsed
views V1 showed (CSP directives with their issues and the Report-Only marker, HSTS fields, the
caching view, CDN detection, the COOP/COEP/CORP values), the CSP verdict follows from structured
issues, Referrer-Policy fallback lists are graded as the specification reads them, the inspection
duration is named for what it measures, and repeated `Set-Cookie` headers stay a list. Closes
inventory rows 4.25, 4.26 and 4.30, the data half of 4.12, 4.23, 4.24, 4.29 and 4.32, and the
RESULT? findings on Referrer-Policy, `response_ms` and `set-cookie`
(`netray-ops/specs/netray/inventory/sections/04-http.md`, private).

## Non-goals

- No template, stylesheet or island change: rendering the new blocks, the speed colouring, the
  weak-ETag and localised Last-Modified formatting and the live inspector's verdicts are the UI half
  (lane A; theme B is lane A's).
- No new check id and no scoring change: COOP, COEP and CORP become evidence, not checks (Decision
  log).
- No change to the V1 wire shape (`InspectResponse` JSON keys) or the V1 frontend. The Referrer-Policy
  fix changes V1's verdict for a fallback list, as intended.
- No CDN detection change (signatures as V1), no new caching verdict.

## Context and constraints

- V1's evaluation already runs in V2's inspection: `inspect/security.rs` grades COOP, COEP and CORP
  (present: pass; missing or not readable: warn "Missing <name> header"), parses HSTS into
  `HstsCheck {max_age, include_sub_domains, preload}`; `inspect/caching.rs` fills `CachingReport`
  (Cache-Control flags, `max_age`, `etag`, `last_modified`, `vary`, `age`) and `CdnReport
  {detected, cache_status, indicators}`; `inspect/csp.rs` parses directives into an ordered map
  and scores them. `module::translate` drops all of it: only the raw header table reaches the
  evidence (`crates/http/src/module.rs` `blocks`).
- V1 graded COOP, COEP and CORP only in its security card, never in `quality.checks`, so V1's
  verdict ignored them (inventory, findings outside the table).
- The profile refuses weight 0 (`crates/engine/src/profile.rs`), SDD §3.8 requires every catalogue
  ID to carry a weight >= 1 in the current profile, and a released profile is never edited: a new
  check ID needs profile v4 and moves every grade.
- `csp::analyze_csp` derives its status from message prefixes (`[fail]`/`[warn]`) and the `csp`
  check's finding is the issue texts joined with `; ` (`quality/checks.rs`); a report-only policy
  warns without any finding saying why. V1's frontend attaches issues to directives by substring
  match on the message (`CspAnalysis.tsx`).
- Referrer-Policy: the W3C algorithm takes the header as a comma-separated list and keeps the last
  token that is a known policy; V2 compares the whole value and warns "Unrecognized value" on
  `origin-when-cross-origin, strict-origin-when-cross-origin`.
- `http.server/response_ms` holds `InspectResponse.duration_ms`: the whole inspection (three
  concurrent probes and the enrichment lookup), as V1's `duration_ms`. lens reads it into its own
  wire field `response_duration_ms` (`crates/lens/src/modules.rs`).
- `headers_block` joins repeated `Set-Cookie` values with `\n` into one row; table rows need one
  value per column and a unique key (`netray_model::evidence::validate`). `http.cookies` already
  uses `#<n>` on a repeated key. A volatile row path matches one row key exactly
  (`crates/model/src/canonical.rs` `table`), and the diff reads the same canonical form.
- CSP nonces are normalised only in a fields key or table row named `content-security-policy` or
  `-report-only` (`canonical.rs` `is_csp`).
- Evidence rules: no placeholder values (an unknown is omitted); a judged check references the block
  holding its deciding fact; a probe without a response gives no block of that response; register
  keys are released once shipped; volatile paths name registered keys
  (`crates/http/tests/evidence.rs`). New blocks change every stored fingerprint once (accepted, as
  in v2-tls-backend).
- Goldens: lens goldens (`tests/fixtures/contracts/lens-*.json`) are built from
  `spectra-inspect.json` and change with the new blocks and findings (`UPDATE_GOLDEN=1`,
  `ADLC-Test-Change`).

## Requirements

1. A Referrer-Policy value is graded by its effective policy: the last comma-separated token,
   trimmed and compared case-insensitively, that is one of the eight policy values. The effective
   policy passes or warns as a single value does today; a permissive one warns "Permissive policy:
   <effective policy>"; a value without a known token warns "Unrecognized value: <value>". This
   holds for the V1 `security.referrer_policy`, the V1 quality check and `http.referrer_policy`.
2. `http.server` carries the inspection's duration as `inspection_ms` (duration) in place of
   `response_ms`, and `http.server/inspection_ms` is volatile in place of
   `http.server/response_ms`. lens fills `response_duration_ms` from `inspection_ms`.
3. `http.headers` holds one row per `Set-Cookie` value in received order: the first keyed
   `set-cookie`, each repeat `set-cookie#<n>` with `n` its ordinal from 2. Other repeated headers
   stay one row joined with `, `.
4. A volatile row path `<block>/<column>@<key>` also covers the rows keyed `<key>#<n>` (`n`
   decimal digits), in the fingerprint and in the diff, so no `Set-Cookie` row is compared.
5. The CSP analysis yields structured issues, each naming its directive and its severity (fail or
   warn). The CSP status follows from the severities and the Report-Only marker, never from message
   text; the V1 `security.csp.issues` texts and order stay as they are.
6. When a Content-Security-Policy or Content-Security-Policy-Report-Only header was received,
   `http.csp` is a tree block: the root holds `report_only` (bool), `issue_count` (int) and an
   `issue` (text, repeated) per issue whose directive the policy lacks; one child per directive in
   policy order holds `directive` (text), a `source` (text, repeated) per source and an `issue`
   (text, repeated) per issue naming that directive. Without either header there is no `http.csp`.
7. `http.csp` references `http.headers` and, when present, `http.csp`. With a policy its findings
   are the issue messages, one finding each in issue order, preceded by "Content-Security-Policy is
   report-only: browsers report violations but enforce nothing" when the policy is report-only.
   Without a policy the finding stays "No Content-Security-Policy header".
8. When a Strict-Transport-Security header was received, `http.hsts` is a fields block with
   `max_age` (int, seconds, when parsed), `include_subdomains` (bool) and `preload` (bool);
   `http.hsts` (the check) references it beside `http.headers`.
9. When the HTTPS probe answered, `http.caching` is a fields block holding, each only when known:
   a `directive` (text, repeated) per Cache-Control flag set (`public`, `private`, `no-store`,
   `no-cache`, `must-revalidate`, `immutable`), `max_age` (int, seconds), `etag` (text),
   `etag_weak` (bool), `last_modified` (time, from the HTTP date; omitted when unparsable), a
   `vary` (text, repeated) per Vary entry and `age` (int, seconds); without any of them the block
   is empty. Cache-Control and Vary are read over every header line. `http.caching` (the check)
   references it beside `http.headers`.
10. When a CDN was detected, `http.cdn` is a fields block with `provider` (text), `cache_status`
    (text, when received) and an `indicator` (text, repeated) per signature header found. Without a
    detected CDN there is no block. No check references it.
11. When the HTTPS probe answered, `http.isolation` is a fields block with `coop`, `coep` and
    `corp` (text, the header value when received and readable) and a `missing` (text, repeated, the
    lowercase header name) for each of the three not received or not readable. No check references
    it.
12. The HTTP register declares every new block and key with its shape and type, and
    `http.caching/etag`, `http.caching/etag_weak`, `http.caching/last_modified`,
    `http.caching/age`, `http.cdn/cache_status` and `http.csp/source` are volatile.
13. SDD `specs/sdd/v2.md` states the renamed key, the `Set-Cookie` rows, the row-repeat rule, the
    new blocks and volatile paths, and COOP/COEP/CORP as evidence, in the same commit as the code.

## Phase 1 — Check fixes: Referrer-Policy lists, inspection duration, Set-Cookie rows

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 13

`crates/http/src/inspect/security.rs` (effective Referrer-Policy), `crates/http/src/module.rs`
(`server_block`, `headers_block`, `REGISTER`, `VOLATILE`), `crates/lens/src/modules.rs`,
`crates/model/src/canonical.rs` (row repeats), catalogue `http.referrer_policy` finding, SDD §3.7.
Lens goldens regenerate.

### Test Scenarios

GIVEN Referrer-Policy `origin-when-cross-origin, strict-origin-when-cross-origin` WHEN analysed THEN `security.referrer_policy` passes with no message and `http.referrer_policy` is pass.
GIVEN Referrer-Policy `no-referrer, unsafe-url` WHEN analysed THEN it warns "Permissive policy: unsafe-url".
GIVEN Referrer-Policy `strict-origin, made-up-policy` WHEN analysed THEN it passes (the unknown last token is skipped).
GIVEN Referrer-Policy `foo, bar` WHEN analysed THEN it warns "Unrecognized value: foo, bar".
GIVEN Referrer-Policy ` Strict-Origin ` WHEN analysed THEN it passes.
GIVEN the fixture response WHEN translated THEN `http.server` holds `inspection_ms` as a duration and no `response_ms`, and the blocks validate.
GIVEN the HTTP module WHEN its volatile list is read THEN it holds `http.server/inspection_ms` and not `http.server/response_ms`.
GIVEN a lens run whose `http.server` carries `inspection_ms` 77 WHEN lens builds its http section THEN `response_duration_ms` is 77.
GIVEN raw headers with three `Set-Cookie` values WHEN translated THEN `http.headers` has rows `set-cookie`, `set-cookie#2` and `set-cookie#3` with one value each, in received order, and the block validates.
GIVEN raw headers with two `Vary` values WHEN translated THEN `http.headers` has one `vary` row joined with `, `.
GIVEN a JSON replay with one `set-cookie` header WHEN translated THEN one `set-cookie` row.
GIVEN two runs whose `Set-Cookie` values differ in every row WHEN fingerprinted and diffed with `http.headers/value@set-cookie` volatile THEN the fingerprints are equal and the diff lists no change.
GIVEN a table row keyed `set-cookiex` or `set-cookie#a` WHEN the path `value@set-cookie` is volatile THEN that row is still compared.

## Phase 2 — Parsed evidence: CSP, HSTS, caching, CDN, isolation headers

**Depends on:** Phase 1
**Requirements:** 5, 6, 7, 8, 9, 10, 11, 12, 13

`crates/http/src/inspect/csp.rs` (structured issues), `crates/http/src/module.rs` (`http.csp`,
`http.hsts`, `http.caching`, `http.cdn`, `http.isolation`, references, `csp` findings, `REGISTER`,
`VOLATILE`), SDD §3.7 and §3.9. Lens goldens regenerate.

### Test Scenarios

GIVEN CSP `script-src 'self'` WHEN analysed THEN the structured issues hold default-src with severity fail and the status is fail; GIVEN `default-src 'self'; script-src 'unsafe-inline'; object-src 'none'; base-uri 'self'` THEN one warn issue on script-src and the status is warn.
GIVEN any CSP WHEN analysed THEN `security.csp.issues` equals the structured issues' messages in order, as the V1 texts.
GIVEN a report-only CSP without issues WHEN analysed THEN the status is warn.
GIVEN the enforced CSP `default-src 'self'; script-src 'self' 'unsafe-inline' https://cdn.example.com` WHEN translated THEN `http.csp` is a tree whose root holds `report_only` false, `issue_count` 2 and the root issue "Missing base-uri directive" (base-uri absent), and whose children are default-src then script-src, script-src holding three `source` values and the issue "script-src contains 'unsafe-inline'"; the block validates.
GIVEN a report-only CSP WHEN translated THEN `http.csp` root holds `report_only` true and the `csp` findings start with the report-only finding.
GIVEN no CSP header WHEN translated THEN there is no `http.csp`, `http.csp` (the check) references `http.headers` only and its finding is "No Content-Security-Policy header".
GIVEN a CSP with two issues WHEN translated THEN the `csp` check holds two findings, each equal to an `issue` in `http.csp`, and references `http.headers` and `http.csp`.
GIVEN Strict-Transport-Security `max-age=31536000; includeSubDomains` WHEN translated THEN `http.hsts` holds `max_age` 31536000, `include_subdomains` true and `preload` false, and the hsts check references `http.headers` and `http.hsts`.
GIVEN Strict-Transport-Security `includeSubDomains` without max-age WHEN translated THEN `http.hsts` holds no `max_age`; GIVEN no such header THEN no `http.hsts`.
GIVEN Cache-Control `public, max-age=3600, must-revalidate`, ETag `W/"abc"`, Last-Modified `Mon, 01 Jan 2024 00:00:00 GMT`, Vary `Accept-Encoding, Accept-Language` and Age `42` WHEN translated THEN `http.caching` holds directives public and must-revalidate, `max_age` 3600, `etag` `W/"abc"`, `etag_weak` true, `last_modified` 2024-01-01T00:00:00Z, two `vary` values and `age` 42, and the caching check references `http.headers` and `http.caching`.
GIVEN a Last-Modified that is no HTTP date WHEN translated THEN `http.caching` holds no `last_modified`; GIVEN no caching header at all THEN `http.caching` is empty and the caching check references `http.headers` and `http.caching`.
GIVEN headers `cf-ray` and `cf-cache-status: HIT` WHEN translated THEN `http.cdn` holds `provider` Cloudflare, `cache_status` HIT and the indicators cf-ray and cf-cache-status; GIVEN no signature header THEN no `http.cdn`.
GIVEN COOP `same-origin` and neither COEP nor CORP WHEN translated THEN `http.isolation` holds `coop` same-origin and `missing` cross-origin-embedder-policy and cross-origin-resource-policy; GIVEN all three THEN no `missing`.
GIVEN a refused HTTPS probe WHEN translated THEN none of `http.csp`, `http.hsts`, `http.caching`, `http.cdn`, `http.isolation` is emitted and the evidence rule holds.
GIVEN the HTTP module WHEN its volatile list is read THEN it holds the five new paths and each names a registered key; GIVEN every new key THEN the register declares it with the type of requirements 6-11.
GIVEN the catalogue and the profile WHEN read THEN the HTTP check IDs are the same twelve as before.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0-7
  (operator, 2026-10-19).
- Independent reading of the Draft skipped: the review step reads the whole range against the spec
  (implementer under the operator's autonomy mandate, 2026-10-19).
- COOP, COEP and CORP as evidence (`http.isolation`), not as check IDs: V1 kept them out of its
  verdict, a weight 0 is refused by the profile loader and SDD §3.8 (every catalogue ID weighted
  >= 1), so a check would need profile v4 and move every grade; V1's evaluation is a function of
  presence, which the block states (value or `missing`) for the UI half to render (implementer
  under the operator's autonomy mandate, 2026-10-19).
- `response_ms` renamed to `inspection_ms`, not re-measured: V1's `duration_ms` measured the same
  whole inspection, and a per-response timing is a new measurement no row asks for. Both keys are
  volatile, so a run that differs only by the rename records as identical and keeps `response_ms` in
  the head entry; the removal shows once, beside the first real change of that domain (implementer
  under the operator's autonomy mandate, 2026-10-19).
- `Set-Cookie` as rows `set-cookie#<n>` over a list value: a table cell holds one value; `#<n>`
  follows `http.cookies`. The row-repeat volatile rule goes into the shared canonical form so the
  existing path `http.headers/value@set-cookie` covers every row (implementer under the operator's
  autonomy mandate, 2026-10-19).
- `http.csp/source` volatile: CSP nonces rotate per response and the canonical nonce rule reads only
  CSP header keys; the policy itself stays compared through the `http.headers` row, nonces
  normalised (implementer under the operator's autonomy mandate, 2026-10-19).
- Issues on absent directives sit on the `http.csp` root, as V1's top-level issue list; the
  attachment comes from the structured issue, not from message text (implementer under the
  operator's autonomy mandate, 2026-10-19).
- The CSP check gains a report-only finding: report-only always warned without a finding saying
  why (implementer under the operator's autonomy mandate, 2026-10-19).
- Caching directives as V1's flags (`public`, `private`, `no-store`, `no-cache`, `must-revalidate`,
  `immutable`) plus `max_age`: the raw Cache-Control stays in `http.headers` (implementer under the
  operator's autonomy mandate, 2026-10-19).
- New blocks also in the domain run: the stored evidence view shows them; the next run of each
  domain records them as added, accepted (implementer under the operator's autonomy mandate,
  2026-10-19).

- Review of Phase 1: Referrer-Policy grades every header line joined with `, ` (RFC 9110 §5.3), not
  only the first; a received header literally named `set-cookie#<n>` keeps its row and the cookie
  takes the next free ordinal (implementer under the operator's autonomy mandate, 2026-10-19).
- Review of Phase 2: `http.caching` stands whenever the HTTPS probe answered (empty without cache
  headers, as V1 listed each as "not set") and `etag_weak` is volatile, so a volatile key alone never
  adds, removes or changes the block in a comparison; Cache-Control and Vary are read over every
  header line (RFC 9110 §5.3), which also feeds V1's caching verdict the whole list; HTTP dates are
  read in all three RFC 9110 forms; integers above i64 are omitted, never wrapped (implementer
  under the operator's autonomy mandate, 2026-10-19).
- Finish: three CLAUDE.md bullets (volatile keys and presence, row repeats, checks outside the verdict) and one
  check to build (list-valued headers read with `get`); no solutions note (implementer under the
  operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

- Rendering of the new blocks, speed colouring, ETag and date formatting, status words per header
  and the live inspector's findings: lane A, `crates/web/templates`, `crates/web/static`,
  `crates/web/islands`.
- IP role and threat badges (4.14), sharing and export (4.7-4.10), keyboard shortcuts and help
  (4.34, 4.35): other themes in `v2-parity.md`.
