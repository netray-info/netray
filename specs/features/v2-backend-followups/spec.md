# Spec: v2-backend-followups

Status: Draft
Created: 2026-10-22

## Goal

Three backend follow-ups proposed by features v2-http-checks, v2-http-ui and v2-tls-ui hold:
`http.caching` records every Cache-Control directive the response sent, in received order;
`http.isolation` tells a COOP, COEP or CORP header that was received but is not readable as text
apart from one that was not received, and the `http.coop` check's finding says which of the two
it judged, with its verdict unchanged; and the Markdown, CSV and JSON exports of a TLS inspection
of several ports carry each port's own checks beside the band port's.

## Non-goals

- No scoring change: `http.coop` keeps weight 1 and warns on a missing or unreadable header as
  today; no check id is added or removed.
- No change to the V1 wire (`InspectResponse` JSON, `CacheControlDirectives`, the V1 security
  report's texts) or to the V1 frontend.
- No template, stylesheet or island change (lane A); the protocol views in
  `crates/web/src/pages/views/http.rs` change only as far as the new evidence needs to render
  without a duplicate or a lost value.
- No stored history rewrite: stored entries keep the six-flag `directive` form and still render.
- No per-port sections in the exports of a one-port TLS run or of any other inspector.

## Context and constraints

- Rust workspace; gate `just adlc-verify` (offline); full suite `just check`. Integration tests go
  into `crates/<c>/tests/it/` (CLAUDE.md).
- `crates/http/src/module.rs` `caching_block` writes `directive` only for V1's six flags
  (`public`, `private`, `no-store`, `no-cache`, `must-revalidate`, `immutable`) in that fixed order;
  `max_age` comes from `CacheControlDirectives`; the raw Cache-Control is read over every header
  line and joined with `, ` (`crates/http/src/inspect/caching.rs` `list_header`) and kept in
  `CachingReport.cache_control`. `s-maxage`, `stale-while-revalidate`, `no-transform`,
  `proxy-revalidate`, quoted arguments and extensions leave no trace in the block.
- `http.caching/etag`, `etag_weak`, `last_modified` and `age` are volatile; `directive` and
  `max_age` are not (`VOLATILE` in `module.rs`). A change of what a block holds changes every
  stored fingerprint it touches: the next run of such a domain stores an entry whose diff shows the
  block changed (CLAUDE.md, "A new volatile path changes every stored fingerprint").
- `isolation_block` writes `coop`/`coep`/`corp` when `HeaderCheck.value` is `Some`, else the
  header name under `missing`. `analyze_header_presence` (`inspect/security.rs`) leaves `value`
  `None` both for a missing header and for one whose first value fails `HeaderValue::to_str`
  (message "Header value contains non-ASCII"); `dump_headers` and `raw_headers` drop unreadable
  values, so nothing structured records that the header was received.
- `coop_verdict` (`module.rs`) warns without a readable value, using the V1 message as finding;
  the catalogue text for `http.coop` already names "a value not readable as text"
  (`crates/engine/catalogue/checks.toml`). Profile v4 weights `http.coop` 1 (SDD S41).
- Internal data on V1 structs is a `#[serde(skip)]` field so the V1 wire and goldens stay
  byte-identical (CLAUDE.md); `InspectResponse` has two struct literals (`inspect/mod.rs`,
  `quality/checks.rs` test).
- The isolation view (`crates/web/src/pages/views/http.rs` `isolation`) claims only `coop`,
  `coep`, `corp`, `missing`; the caching view appends a synthetic `max-age=<n>` chip from
  `max_age`. A view claims a block only when it renders every key, and each view keeps
  `views_nothing_lost.rs` rows (CLAUDE.md).
- Exports (`crates/web/src/export.rs`): `ExportInput.band` is the band port's checks;
  `crates/web/src/pages/inspect.rs` `export_html` builds the input in the live run and in
  `shared_html`; per-part checks live in `Run.part_checks` / `SharedRun.part_checks`, and
  `panel_names` names the ports (label without `port `) when more than one part carries checks
  (`api/jobs.rs` `port_jobs`: label `port <n>` with several ports).
- SDD `specs/sdd/v2.md` §3.9 http row states the six-flag `directive` and `missing` "for each not
  received or not readable": amended in the same commit as the code.

## Requirements

1. When the HTTPS probe answered with a Cache-Control header, `http.caching` holds one `directive`
   (text) per Cache-Control directive over every header line, in received order, repeats kept: the
   directive name lowercased, followed by `=` and its argument as received (trimmed, quotes kept)
   when it has one, e.g. `max-age=3600`, `no-cache="set-cookie"`, `stale-while-revalidate=30`.
   A comma inside a quoted argument does not split it; empty list members are skipped.
   `max_age` stays as today. Without a Cache-Control header there is no `directive`.
2. The caching view shows each stored `directive` as a chip in stored order and adds the
   `max-age=<n>` chip from `max_age` only when no stored directive is a `max-age` directive, so a
   new entry shows max-age once and a stored six-flag entry renders as before.
3. `http.isolation` writes, for each of COOP, COEP and CORP, exactly one of: the value (`coop`,
   `coep`, `corp`) when the first header line is readable as text; the lowercase header name under
   a new repeated text key `unreadable` when the header was received but its first line is not
   readable as text; the lowercase header name under `missing` when it was not received. The
   register declares `http.isolation/unreadable` as text.
4. The `http.coop` check keeps its status for every input (warn for a missing or unreadable
   header, as today); for an unreadable header its finding is "Cross-Origin-Opener-Policy header is
   not readable as text"; the missing finding and every readable-value branch are unchanged.
   After a JSON replay, which carries no record of unreadable headers, the block and finding read
   as missing, as today.
5. The isolation view claims a block holding `unreadable` and shows "unreadable" (no tone for COEP
   and CORP; COOP in the `http.coop` tone) for that header; a block naming one header under two of
   `missing`, `unreadable` or a value, or an unknown name under `unreadable`, is not claimed.
6. The Markdown, CSV and JSON exports of a TLS run with more than one port carrying its own checks
   keep the band port's checks as today and add each such port's checks, in input order:
   Markdown a section "Checks · port <p>" with the same table as "Checks"; CSV rows in section
   `checks@<p>` with the same item, field and value columns as `checks`; JSON a key `ports`, a list
   of objects `{"port": "<p>", "checks": [...]}`. A port whose checks ended in an error adds the
   error row `checks@<p>` with its text instead (Rule M). The live run's export and a shared run's
   export are equal for the same run.
7. A one-port TLS run and every other inspector's exports are byte-identical to today's.
8. SDD §3.9's http row states requirements 1 and 3; §6.1 states requirement 6.

## Phase 1 — HTTP evidence: every Cache-Control directive, unreadable isolation headers

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 8

`crates/http/src/module.rs` (`caching_block` reads `CachingReport.cache_control` with a quote-aware
splitter; `isolation_block`, `coop_verdict`; `REGISTER`), `crates/http/src/inspect/assembler.rs`
and `inspect/mod.rs` (a `#[serde(skip)] unreadable_headers` on `InspectResponse`),
`crates/web/src/pages/views/http.rs` (caching chip, isolation claim), lens goldens regenerated when
they change, SDD §3.9 and §6.1 (exports sentence lands with Phase 2).

### Test Scenarios

GIVEN Cache-Control `public, max-age=3600, must-revalidate` WHEN translated THEN `http.caching` directives are `public`, `max-age=3600`, `must-revalidate` in that order and `max_age` is 3600.
GIVEN two Cache-Control lines `Max-Age=60, s-maxage=600` and `stale-while-revalidate=30, no-transform` WHEN translated THEN the directives are `max-age=60`, `s-maxage=600`, `stale-while-revalidate=30`, `no-transform`.
GIVEN Cache-Control `no-cache="set-cookie, x-trace", private` WHEN translated THEN the directives are `no-cache="set-cookie, x-trace"` and `private`.
GIVEN Cache-Control `public, , no-store,` WHEN translated THEN the directives are `public` and `no-store` and the block validates.
GIVEN no Cache-Control header but an ETag WHEN translated THEN `http.caching` holds no `directive`.
GIVEN a stored caching block with directives `public`, `must-revalidate` and `max_age` 3600 WHEN the view renders THEN the Cache-Control cell holds three chips, the last `max-age=3600`.
GIVEN a caching block with directives `public`, `max-age=3600` and `max_age` 3600 WHEN the view renders THEN `max-age=3600` appears once in the Cache-Control cell.
GIVEN COOP whose value holds a non-ASCII byte and no COEP or CORP WHEN translated THEN `http.isolation` holds `unreadable` cross-origin-opener-policy and `missing` cross-origin-embedder-policy and cross-origin-resource-policy, no `coop`, and the block validates.
GIVEN COEP and CORP received unreadable and COOP `same-origin` WHEN translated THEN `coop` same-origin and `unreadable` for the other two, no `missing`.
GIVEN all three headers readable WHEN translated THEN neither `missing` nor `unreadable` appears.
GIVEN an unreadable COOP WHEN checked THEN `http.coop` warns with the finding "Cross-Origin-Opener-Policy header is not readable as text"; GIVEN no COOP THEN it warns with "Missing cross-origin-opener-policy header"; GIVEN `same-origin` THEN it passes.
GIVEN an unreadable COOP WHEN the V1 response is serialized THEN `security.coop` is unchanged (status warn, message "Header value contains non-ASCII") and no new JSON key appears.
GIVEN an isolation block with `unreadable` cross-origin-embedder-policy WHEN the view renders THEN the COEP cell says "unreadable" without a tone; GIVEN `unreadable` cross-origin-opener-policy and a failing `http.coop` THEN the COOP cell says "unreadable" in the check's tone.
GIVEN an isolation block naming cross-origin-opener-policy under both `missing` and `unreadable`, or an unknown name under `unreadable` WHEN rendered THEN the view does not claim it.

## Phase 2 — Per-port checks in multi-port TLS exports

**Depends on:** Phase 1
**Requirements:** 6, 7

`crates/web/src/export.rs` (`ExportInput` gains the per-port checks and their errors; Markdown,
CSV, JSON), `crates/web/src/pages/inspect.rs` (`export_html` fed from `part_checks` in the live
and the shared run, only when `panel_names` names ports), SDD §6.1.

### Test Scenarios

GIVEN a TLS run of ports 443 and 465 whose 465 checks differ from 443's WHEN exported as JSON THEN `checks` holds the band port's checks and `ports` holds two objects, port "443" then "465", each with its own checks.
GIVEN the same run WHEN exported as CSV THEN rows `checks@443` and `checks@465` follow the `checks` rows, and port 465's status row of a check reads 465's status.
GIVEN the same run WHEN exported as Markdown THEN sections "Checks · port 443" and "Checks · port 465" follow "Checks", each a table with the port's statuses.
GIVEN a two-port run whose port 465 checks ended in "timed out" WHEN exported THEN CSV holds the row `checks@465,,error,timed out`, Markdown "Not measured: timed out" under "checks@465", JSON an `errors` entry with section `checks@465`, and no checks of 465.
GIVEN a one-port TLS run WHEN exported THEN the three exports equal today's (no `ports` key, no `checks@` rows, no per-port section).
GIVEN a DNS or HTTP inspector run WHEN exported THEN the three exports equal today's.
GIVEN a finished two-port run and its shared copy WHEN both exports are rendered THEN they are equal.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate (operator,
  2026-10-19, extended 2026-10-22).
- Every directive as `name` or `name=argument` in one repeated `directive` key over a separate
  key per argument-carrying directive: one key keeps received order and repeats, and the stored
  text is what the server sent; it supersedes v2-http-checks' six-flag decision. `max_age` stays
  so the view's duration cell and existing readers keep working (implementer under the operator's
  autonomy mandate, 2026-10-22).
- A one-time fingerprint change accepted: a domain whose Cache-Control held `max-age` or a
  directive outside the six flags, or listed the flags in another order, stores a new entry on its
  next run whose diff shows `http.caching` changed; the same holds for a site with an unreadable
  COOP/COEP/CORP (`missing` becomes `unreadable`). No grade changes (implementer under the
  operator's autonomy mandate, 2026-10-22).
- `http.coop` keeps warn for an unreadable header and only its finding names the case: failing or
  passing it would shift grades silently under profile v4; the catalogue text already covers the
  branch (implementer under the operator's autonomy mandate, 2026-10-22).
- Unreadable means the first header line fails `to_str`, the same line V1's security report reads,
  so the evidence and the check judge one fact; recorded through a `#[serde(skip)]`
  `unreadable_headers` on `InspectResponse`, never from the V1 message text (implementer under the
  operator's autonomy mandate, 2026-10-22).
- Exports keep the band port's `checks` and add per-port groups over replacing `checks`: existing
  readers of `checks` keep working, and every port, the band port included, can be read under its
  own name. Port names are text as the tabs name them (implementer under the operator's autonomy
  mandate, 2026-10-22).
- The two view changes (caching chip, isolation `unreadable`) are made here although lane A owns
  the UI: they are Rust view logic, not templates, and without them a new entry would show max-age
  twice and an isolation block with `unreadable` would fall back to the generic block (implementer
  under the operator's autonomy mandate, 2026-10-22).

## Open decisions

None.

## Out of scope

- Parsing Expires or Pragma into `http.caching`: they stay in `http.headers`.
- A new caching verdict over the new directives: `crates/http/src/quality/checks.rs` unchanged.
- Styling of the "unreadable" cell or a per-port export UI: lane A, `crates/web/templates`,
  `crates/web/static`, `crates/web/islands`.
- The private inventory rows: `netray.info/specs/inventory/v2-parity.md` (private).
