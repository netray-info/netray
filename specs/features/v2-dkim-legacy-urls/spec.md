# Spec: v2-dkim-legacy-urls

Status: Ready for Implementation
Created: 2026-10-19

## Goal

The backend halves of inventory themes I and J (private inventory, `v2-parity.md`) hold. Custom
DKIM selectors are accepted again on every surface that runs DKIM: a domain run started from the
result page (`POST /<domain>/run`, `GET /<domain>?dkim_selectors=`, `/?d=` with
`dkim_selectors`), API v1's `POST /domains/{domain}/runs` and `POST /email/inspect`, and the
OpenAPI document; the email module checks the given selectors. Old V1 site URLs answer `301` to
their V2 page instead of `404`: `/guide/<slug>.html`, `/compare/`, `/status/`, `/tools`,
`/tools/`. `/guide/scoring` describes the parameter as the code implements it.

Closes inventory rows 1.31, 5.2, 7.71 (backend half), 7.37, 7.42.

## Non-goals

- No visible selector input on any page: the inspector shell and result page forms are lane A's
  (UI half of theme I). This feature carries the list through a hidden field only.
- No email inspector page (S3 gives email no UI; `Tool` stays dns, tls, http, ip).
- No change to the `email.` host redirect: it lives on `feature/v2-cutover` (read-only here); the
  rule it must follow is fixed in the Decision log and in SDD §7.3.
- No `/tools` page (row 7.44), no header or footer links (7.58, 7.59, 7.65, 7.66), no sitemap
  change (7.50).
- No change to how a run without selectors is admitted, cached, joined or recorded.

## Context and constraints

- V1: lens `GET /api/check/{domain}?dkim_selectors=a,b` and `POST /api/check {dkim_selectors}`
  (comma string, at most 10, `[a-zA-Z0-9-]`, 1-63 chars, `crates/lens/src/routes.rs:65`); beacon
  `POST /inspect {domain, dkim_selectors: [..]}` and its UI `?q=<domain> <sel>…` (at most
  `dkim.max_user_selectors`, default 5, `crates/email/src/config.rs:370`). lens forwarded the list
  to beacon, which refused more than 5, so the effective V1 limit of a domain run was 5.
- V2 today: `crates/lens/src/admission.rs:380` builds every admitted run with
  `dkim_selectors: None`; `EmailInspectRequest` is `deny_unknown_fields` and refuses
  `dkim_selectors` (`crates/web/tests/it/api_email.rs:250` pins that); `POST
  /domains/{domain}/runs` takes no body and ignores one.
- The run path that already carries selectors: `CheckInput.dkim_selectors` →
  `BackendContext.dkim_selectors` → `RunOptions.dkim_selectors` → the email module
  (`crates/email/src/module.rs:218`, which validates against its own config and reports an
  invalid list as an incomplete section). lens's `admit_dkim` (`admission.rs:408`) is V1's
  selector path: not joinable, never joins, throttle, slot and budget, and `history::record`
  skips it (`history.rs:114`), so it is never recorded nor held in the cache.
- `DomainRuns` (`crates/web/src/api/runs.rs`) is the seam the result page and API v1 run through;
  about 60 test stubs implement it. `feature/v2-cutover` copies lens's implementation into
  `crates/web/src/admission/runs.rs` (`AdmittedRuns`).
- The streaming island posts every field of the recheck form (`crates/web/islands/src/stream.ts:95`),
  so a field in that form reaches `POST /<domain>/run` with and without JavaScript.
- `/guide/scoring` (`crates/web/templates/pages/guides/scoring.html:213-240, 274-281`) documents
  `?d=…&dkim_selectors=…`, `POST …/runs?dkim_selectors=…`, a count of 1-10, a jq path `.checks[]`
  that the run answer does not have (it is `.run.checks[]`), and V1 cache wording.
- V1 site paths (inventory evidence `site-api/site-paths.txt`): `/guide/<slug>.html` (all 32 V1
  guides, `lens.html` and `index.html` included), `/compare/`, `/status/`, `/tools`, `/tools/`
  answered 200. V2's guide slugs cover every V1 slug except `lens` (now `scoring`) and `index`
  (the index is `/guide/`). The inspector index is the home page `/` today (its "One protocol
  only" links).

## Requirements

1. One selector-list rule for every comma-form surface (API query, page query, form field):
   the value is split on commas, whitespace around each token is trimmed; a value that is empty
   or only whitespace means no selectors; otherwise it holds 1 to 5 tokens, each 1-63 characters
   of ASCII letters, digits and hyphens, and no empty token. A list that breaks the rule is
   refused before any run starts. The DKIM check probes a selector given twice, or in another
   case, once (amended after the Phase 2 review).
2. `POST /api/v1/domains/{domain}/runs?dkim_selectors=<list>` starts a selector run (requirement
   4) with the list; a list breaking requirement 1 answers `400 INVALID_INPUT` and starts
   nothing; without the parameter the endpoint behaves as today. The OpenAPI document lists the
   optional query parameter `dkim_selectors`.
3. `POST /api/v1/email/inspect` accepts an optional `dkim_selectors` array beside `domain`; the
   inspection's DKIM checks probe those selectors; more than the email config's
   `max_user_selectors` (default 5) or an invalid selector answers `400 INVALID_INPUT`; other
   unknown fields still answer `400`. The OpenAPI schema of `EmailInspectRequest` has the field.
4. A selector run, through lens's `DomainRuns`, carries the list to the email module, whose DKIM
   check then reports those selectors. It never joins a run in flight and is never joined, is
   never answered from a stored or cached result, and is never recorded in the history (its
   answer says `recorded: not_stored`, `entry_id: null`). It passes the per-IP and global limits,
   the domain throttle, a slot and a budget token, and starts the domain's throttle minute. A
   prefetch starts no selector run.
5. `POST /<domain>/run` accepts the form field `dkim_selectors`. A valid non-empty list starts a
   selector run for both triggers (a visit skips the fresh-history answer); an invalid list
   answers the `400` refusal page. With `Accept: text/event-stream` the run streams as any run;
   without it the response is the finished run's result page (`200`, `Cache-Control: no-store`)
   instead of the `303` to `/<domain>`, which would show the history without this run.
6. `GET /<domain>?dkim_selectors=<list>` with a valid non-empty list answers the result page with
   `Cache-Control: no-store`, starting a run on load even when the head entry is fresh, and the
   recheck form carries the normalised list (tokens joined by commas) in a hidden
   `dkim_selectors` field; an invalid list answers the `400` refusal page. Without the parameter,
   or with an empty one, the page is unchanged.
7. `GET /?d=<domain>&dkim_selectors=<value>` with a non-empty value answers `301` to
   `/<domain>?dkim_selectors=<value>` (the value percent-encoded); without it the redirect is
   unchanged.
8. `/guide/scoring` states what the code does: count 1-5, the API sample reads
   `.run.checks[]`, and a run with `dkim_selectors` is never answered from a stored result, never
   joined and never stored.
9. `GET /guide/<slug>.html` answers `301` to `/guide/<slug>` for every guide slug;
   `/guide/index.html` to `/guide/`; `/guide/lens.html` and `/guide/lens` to `/guide/scoring`; a
   `.html` stem that names no guide answers `404`.
10. `GET /compare/` answers `301` to `/compare`, `GET /status/` to `/status`, `GET /tools` and
    `GET /tools/` to `/`.
11. Every production `impl DomainRuns` overrides the selector-carrying start method, checked by a
    repository test, so a copy of the implementation cannot drop selectors silently.
12. `specs/sdd/v2.md` states requirements 2-4, 7, 9 and 10 and the `email.` redirect rule.

## Phase 1 — Old site URLs answer 301

**Depends on:** none
**Requirements:** 9, 10

Routes in `netray_web::pages::router` (guides, compare, status, tools); SDD §7.3 gains the rows.

### Test Scenarios

GIVEN the pages router WHEN `GET /guide/asn.html` THEN `301` with `Location: /guide/asn`.
GIVEN every slug of `GUIDES` WHEN `GET /guide/<slug>.html` THEN `301` to `/guide/<slug>`.
GIVEN the pages router WHEN `GET /guide/index.html` THEN `301` to `/guide/`.
GIVEN the pages router WHEN `GET /guide/lens.html` or `GET /guide/lens` THEN `301` to `/guide/scoring`.
GIVEN the pages router WHEN `GET /guide/nonesuch.html` THEN `404`.
GIVEN the pages router WHEN `GET /compare/` THEN `301` to `/compare`; WHEN `GET /status/` THEN `301` to `/status`.
GIVEN the pages router WHEN `GET /tools` or `GET /tools/` THEN `301` to `/`.
GIVEN the pages router WHEN `GET /guide/asn` THEN `200` as before.

## Phase 2 — Selectors in API v1 and the run seam

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 11, 12

`DomainRuns` gains a start method that carries selectors (default: the plain start when the list
is empty, for test stubs); lens's `LensRuns` overrides it over `admit_dkim`. The parser of
requirement 1 lives in `netray-web`. `EmailInspectRequest` gains the field. The OpenAPI golden
grows compatibly.

### Test Scenarios

GIVEN a stub `DomainRuns` WHEN `POST /domains/example.com/runs?dkim_selectors=google,%20s1` THEN `200` and the stub's selector start received `["google","s1"]`.
GIVEN a stub WHEN `POST /domains/example.com/runs?dkim_selectors=bad.sel` THEN `400 INVALID_INPUT` and the stub saw no call.
GIVEN a stub WHEN the list has 6 tokens THEN `400 INVALID_INPUT`; WHEN it has 5 THEN `200`.
GIVEN a stub WHEN `dkim_selectors=a,,b` THEN `400 INVALID_INPUT`; WHEN a token has 64 characters THEN `400`; WHEN it has 63 THEN `200`.
GIVEN a stub WHEN `dkim_selectors=` (empty) THEN `200` and the plain start ran.
GIVEN a stub WHEN no parameter THEN the plain start ran as before.
GIVEN the OpenAPI document WHEN read THEN `POST /domains/{domain}/runs` lists an optional query parameter `dkim_selectors`, and `EmailInspectRequest` has a `dkim_selectors` array property.
GIVEN the stub email inspector WHEN `POST /email/inspect {"domain":"example.com","dkim_selectors":["s1"]}` THEN `200`.
GIVEN the stub email inspector WHEN `dkim_selectors` holds 6 selectors or `"bad.sel"` THEN `400 INVALID_INPUT`; WHEN the body has another unknown field THEN `400`.
GIVEN an email inspection with `dkim_selectors: ["custom1"]` over stub DNS publishing a key at `custom1._domainkey` WHEN collected THEN the `email.dkim` evidence holds a `custom1#…` row.
GIVEN lens with a stub check WHEN two selector starts of one domain arrive within the throttle minute THEN the second is refused `throttle`; WHEN a selector start follows a plain run in flight THEN it is refused `throttle` and does not join it.
GIVEN lens WHEN a selector run finishes complete THEN its stream ends with `Recorded(None)` and the history holds no entry for the domain.
GIVEN lens WHEN a selector start is a prefetch THEN `NotStarted`.
GIVEN lens WHEN a selector run runs THEN the email module's `RunContext.options.dkim_selectors` is the list.
GIVEN the repository WHEN `tests/repo/test_domain_runs_selectors.sh` runs THEN it passes, and it fails on a planted `impl DomainRuns` without the override.

## Phase 3 — Selectors on the result page

**Depends on:** Phase 2
**Requirements:** 5, 6, 7, 8, 12

### Test Scenarios

GIVEN a stub `DomainRuns` and a fresh head entry WHEN `POST /example.com/run` with `trigger=visit&dkim_selectors=s1` and `Accept: text/event-stream` THEN the stub's selector start received `["s1"]` (the fresh-history answer is skipped) and the stream ends.
GIVEN a stub WHEN `POST /example.com/run` with `trigger=recheck&dkim_selectors=bad.sel` THEN `400` and no start.
GIVEN a stub WHEN `trigger=visit&dkim_selectors=` THEN the plain path runs as before.
GIVEN a stub answering a complete unrecorded run WHEN `POST /example.com/run` with `trigger=recheck&dkim_selectors=s1` without SSE THEN `200`, `Cache-Control: no-store`, and the page shows the run's grade and "not recorded".
GIVEN a fresh head WHEN `GET /example.com?dkim_selectors=s1,%20s2` THEN `200`, `Cache-Control: no-store`, the recheck form has `data-stream-autostart="visit"` and a hidden input `name="dkim_selectors" value="s1,s2"`.
GIVEN WHEN `GET /example.com?dkim_selectors=bad.sel` THEN `400`.
GIVEN WHEN `GET /example.com` THEN no `dkim_selectors` input and the page as before.
GIVEN WHEN `GET /?d=example.com&dkim_selectors=google,s1` THEN `301` to `/example.com?dkim_selectors=google%2Cs1` (or the unencoded comma); WHEN `GET /?d=example.com` THEN `301` to `/example.com`.
GIVEN `/guide/scoring` WHEN rendered THEN it says 1-5 selectors, its sample reads `.run.checks[]`, and it no longer says "Results are cached per domain for 5 minutes" beside the selector rule.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7 (operator, 2026-10-19).
- Limit 5, not lens V1's 10: V1's effective limit for a domain run was beacon's 5 (lens forwarded, beacon refused more), the email module still validates against `max_user_selectors` (default 5), and accepting 6-10 would start runs whose email section ends incomplete (implementer under the operator's autonomy mandate, 2026-10-19).
- Empty value means no selectors on every surface, also the API (V1's API answered 400): an empty visible input submits `dkim_selectors=`, and V1 API clients are ignored per the operator's theme I decision (implementer under the operator's autonomy mandate, 2026-10-19).
- Caching and history: a run with custom selectors answers a different question than the stored head, so it is not cached, not joined, never joins, and not recorded (V1's `admit_dkim` semantics, SDD §4.4 already says so); it shares the domain's throttle minute and spends a budget token, which keeps one target from being probed faster than once a minute (implementer under the operator's autonomy mandate, 2026-10-19).
- API runs take the list as a query parameter (V1 lens's `GET /api/check?dkim_selectors=`, and what `/guide/scoring` documents); the endpoint keeps taking no body. Email inspect takes a JSON array in the body (V1 beacon's `POST /inspect` shape) (implementer under the operator's autonomy mandate, 2026-10-19).
- `email.` redirect (built on `feature/v2-cutover`): `/?q=<domain> <sel1> <sel2>…` with at least one selector token answers `301` to `https://<apex>/<domain>?dkim_selectors=<sel1>,<sel2>` (the run page, which can run them), not `/<domain>/email` (a stored read that cannot); without selector tokens it stays `/<domain>/email`; invalid tokens are carried and refused by the target's `400`. SDD §7.3 is amended here so the cutover implements this rule (implementer under the operator's autonomy mandate, 2026-10-19).
- The trait method has a default that delegates to the plain start so the ~60 test stubs compile unchanged; the repo check of requirement 11 keeps production implementations, the cutover's `AdmittedRuns` included, from inheriting that default (implementer under the operator's autonomy mandate, 2026-10-19).
- Minimal template changes, outside the brief's "no templates" rule, logged: one hidden input in `partials/result_header.html` (requirement 6 needs the form to carry the list), and the guide text in `pages/guides/scoring.html` (the task asks code and guide to agree; the guide's jq path and count are wrong in any case). Visible inputs stay with lane A, which replaces the hidden field (implementer under the operator's autonomy mandate, 2026-10-19).
- Phase 2 review: a repeated or case-variant selector produced a second `email.dkim` row for one DNS record; the DKIM check now compares candidates case-insensitively (requirement 1 amended). The fixed limit of 5 versus a configured `max_user_selectors` below 5 is deferred: production sets 5 (implementer under the operator's autonomy mandate, 2026-10-19).
- `/tools` goes to `/`: the home page holds the inspector links today; a `/tools` page (7.44) would take the redirect over (implementer under the operator's autonomy mandate, 2026-10-19).
- Relative `Location` headers, as the existing `/guide` → `/guide/` redirect; the cutover's host layer may answer the same paths first with absolute apex URLs (implementer under the operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

- The selector inputs on the home, result and inspector pages and their copy (lane A).
- "not recorded" as the run line of a selector run: copy for lane A.
- The `email.` host redirect code (feature/v2-cutover).
