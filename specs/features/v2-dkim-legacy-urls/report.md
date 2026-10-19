# Report: v2-dkim-legacy-urls

## Phase 1 — Old site URLs answer 301

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R9: `/guide/<slug>.html` 301 to `/guide/<slug>`, index.html to `/guide/`, lens and lens.html to `/guide/scoring`, unknown stem 404 | green | crates/web/tests/it/pages_legacy_urls.rs |
| C2 | R10: `/compare/` to `/compare`, `/status/` to `/status`, `/tools` and `/tools/` to `/` | green | crates/web/tests/it/pages_legacy_urls.rs |
| C3 | GET /guide/asn.html: 301 Location /guide/asn | green | crates/web/tests/it/pages_legacy_urls.rs |
| C4 | every GUIDES slug: /guide/<slug>.html 301 to /guide/<slug> | green | crates/web/tests/it/pages_legacy_urls.rs |
| C5 | GET /guide/index.html: 301 to /guide/ | green | crates/web/tests/it/pages_legacy_urls.rs |
| C6 | GET /guide/lens.html and /guide/lens: 301 to /guide/scoring | green | crates/web/tests/it/pages_legacy_urls.rs |
| C7 | GET /guide/nonesuch.html: 404 | already_implemented | crates/web/tests/it/pages_legacy_urls.rs |
| C8 | GET /compare/ 301 /compare; GET /status/ 301 /status | green | crates/web/tests/it/pages_legacy_urls.rs |
| C9 | GET /tools and /tools/: 301 / | green | crates/web/tests/it/pages_legacy_urls.rs |
| C10 | GET /guide/asn: 200 as before | already_implemented | crates/web/tests/it/pages_legacy_urls.rs |

C7 and C10 passed before the change: a 404 for an unknown stem and 200 for a V2 slug already held; the tests stay to protect them.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 guides.rs, pages/mod.rs | 1 | implementer (no coder spawned: two-file change) | 0 | 0 |

### Review

Reader: no findings on the code; one NIT in the SDD row (the V1 sitemap lists `/tools/`, not `/tools`), fixed in the row's wording.

### Behavioural verification

`netray lens lens.dev.toml` on 127.0.0.1:18791, `curl -s -o /dev/null -w '%{http_code} %header{location}'`:

```
/guide/asn.html 301 /guide/asn
/guide/lens.html 301 /guide/scoring
/guide/index.html 301 /guide/
/guide/nonesuch.html 404
/guide/asn 200
/compare/ 301 /compare
/status/ 301 /status
/tools 301 /
/tools/ 301 /
```

## Phase 2 — Selectors in API v1 and the run seam

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: one selector-list rule (comma split, trim, empty means none, 1-5 tokens of 1-63 `[A-Za-z0-9-]`, no empty token) | green | crates/web/tests/it/api_runs_selectors.rs |
| C2 | R2: runs endpoint takes `dkim_selectors` query, 400 INVALID_INPUT on a bad list, OpenAPI lists it | green | crates/web/tests/it/api_runs_selectors.rs |
| C3 | R3: email inspect takes a `dkim_selectors` array, 400 over the limit or invalid, OpenAPI schema has it | green | crates/web/tests/it/api_email_selectors.rs |
| C4 | R4: a selector run carries the list to the email module, never joins nor is joined, is not recorded, passes throttle, slot and budget, no prefetch | green | crates/lens/tests/it/admission_selectors.rs |
| C5 | R11: every production `impl DomainRuns` overrides `start_with` | green | tests/repo/test_domain_runs_selectors.sh |
| C6 | R12: the SDD states requirements 2-4 and the `email.` redirect rule | test-unwritable | (read) |
| C7 | runs `?dkim_selectors=google,%20s1`: 200, selector start got google and s1 | green | crates/web/tests/it/api_runs_selectors.rs |
| C8 | runs `?dkim_selectors=bad.sel`: 400 INVALID_INPUT, no call | green | crates/web/tests/it/api_runs_selectors.rs |
| C9 | 6 tokens 400, 5 tokens 200 | green | crates/web/tests/it/api_runs_selectors.rs |
| C10 | `a,,b` 400; 64-char token 400; 63-char 200 | green | crates/web/tests/it/api_runs_selectors.rs |
| C11 | empty `dkim_selectors=`: 200 and the plain start ran | green | crates/web/tests/it/api_runs_selectors.rs |
| C12 | no parameter: plain start as before | green | crates/web/tests/it/api_runs_selectors.rs |
| C13 | OpenAPI: runs lists optional query `dkim_selectors`; EmailInspectRequest has a `dkim_selectors` array | green | crates/web/tests/it/api_runs_selectors.rs |
| C14 | email inspect with `["s1"]`: 200 | green | crates/web/tests/it/api_email_selectors.rs |
| C15 | email inspect with 6 selectors or `bad.sel`: 400 INVALID_INPUT; another unknown field 400 | green | crates/web/tests/it/api_email_selectors.rs |
| C16 | email inspection with `["custom1"]` over stub DNS with a key at `custom1._domainkey`: `email.dkim` evidence holds a `custom1#` row | green | crates/email/tests/it/inspect_selectors.rs |
| C17 | lens: second selector start in the throttle minute refused throttle; selector start beside a plain run in flight refused throttle, not joined | green | crates/lens/tests/it/admission_selectors.rs |
| C18 | lens: complete selector run ends `Recorded(None)`, no history entry | green | crates/lens/tests/it/admission_selectors.rs |
| C19 | lens: selector prefetch is NotStarted | green | crates/lens/tests/it/admission_selectors.rs |
| C20 | lens: the email module's `RunContext.options.dkim_selectors` is the list | green | crates/lens/tests/it/admission_selectors.rs |
| C21 | repo check passes, and fails on a planted `impl DomainRuns` without the override | green | tests/repo/test_domain_runs_selectors.sh |

C6 is prose: SDD §4.4, §7.1 (runs, email inspect) and §7.3 (`?d=`, `email.`) were amended and read; `tests/repo/test_api_sdd.sh` and `test_sdd_admission.sh` stay green.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 web selectors + runs handler + email request | 1 | implementer (no coder spawned) | 0 | 0 |
| G2 lens `start_with` | 1 | implementer (no coder spawned) | 0 | 0 |
| G3 review fix: DKIM candidate dedupe | 1 | implementer | 0 | 0 |

Test fixes after RED: the `api_runs_selectors.rs` stub answered an empty stream (500 "the run ended without an answer"); it now answers `Done` and `Recorded(None)`. Two struct literals (`crates/email/tests/job.rs`, `crates/web/tests/it/api_email_limits.rs`) gained `dkim_selectors: Vec::new()`. The OpenAPI golden grew compatibly (`UPDATE_GOLDEN=1`).

### Review

- AMENDMENT (repaired in phase): repeated or case-variant selectors gave a second `email.dkim` row for one record; fixed in `checks/dkim.rs`, test `a_repeated_selector_is_probed_once`.
- DEFERRED: the limit 5 does not follow a configured `max_user_selectors` below 5 (production sets 5).
- DEFERRED: SDD §4.4 and §7.3 describe the result-page path built in Phase 3.
- NIT: `/email/inspect`'s 400 description does not name selector errors.
- NIT: trailers for the three changed protected test files (carried by the phase commit).

### Behavioural verification

skipped here: lens's live run path needs outbound DNS, blocked locally (Little Snitch); the selector path is covered offline by `admission_selectors.rs` through the real `LensRuns` and admission, and the endpoint end to end is verified in Phase 3 together with the page.
