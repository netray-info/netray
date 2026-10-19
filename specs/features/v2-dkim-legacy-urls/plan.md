## Phase 1 — Old site URLs answer 301

## Groups

- G1: C1-C10 (crates/web/src/pages/guides.rs, crates/web/src/pages/mod.rs)

## Plan

- G1: `guides::show` maps a legacy slug through `legacy_target` before the lookup; `guides::moved` answers the 301; `pages::router` adds `/compare/`, `/status/`, `/tools`, `/tools/`. SDD §7.3 gains two rows.

## Phase 2 — Selectors in API v1 and the run seam

## Groups

- G1: C1-C3, C7-C15 (crates/web/src/selectors.rs, crates/web/src/api/runs.rs, crates/email/src/job.rs)
- G2: C4, C5, C17-C21 (crates/lens/src/api_v1.rs)
- G3: review fix (crates/email/src/checks/dkim.rs)

## Plan

- G1: `selectors::parse_dkim_selectors` holds requirement 1; `DomainRuns::start_with` with a default; `start_run` reads `RunQuery` and calls `start_with`; `EmailInspectRequest.dkim_selectors` passed to `EmailInspector::start`.
- G2: `LensRuns::start_with`: empty list to `start`; else limits, prefetch, `admission::admit_dkim`.
- G3: the DKIM candidate list compares selectors case-insensitively.

## Phase 3 — Selectors on the result page

## Groups

- G1: C1, C2, C5-C11 (crates/web/src/pages/result/mod.rs, run.rs, templates/partials/result_header.html)
- G2: C3, C4, C12, C13 (crates/web/src/pages/errors.rs, templates/pages/guides/scoring.html)
- G3: review fix (crates/web/src/pages/result/mod.rs)

## Plan

- G1: `show` reads `dkim_selectors` (`selectors_of`), `full_page`/`stored_parts`/`assemble` carry the normalised list into `HeaderPart`; `run` reads the form field, skips the fresh-history answer with selectors, calls `start_with`, and without SSE renders `finished_page` over `done_parts` (factored from `Live::finish`).
- G2: `query_redirect` appends a non-empty `dkim_selectors`; the guide states 1-5, `.run.checks[]` and the not-stored rule.
- G3: `show` passes the query as the redirect tail.
