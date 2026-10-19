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
