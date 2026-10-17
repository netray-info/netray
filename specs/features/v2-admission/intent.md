# Intent: v2-admission

## Problem

lens has two sources of truth for "has this domain been checked recently": the moka cache
(`Cache::builder` in `crates/lens/src/state.rs:87-93`, 300 s) and, since `v2-history`, the history
(`history.db`). Badge, OG and `/api/check` read only moka. Nothing stops a domain from being run
again and again: `/api/check` without a fresh moka entry always starts a run
(`crates/lens/src/routes.rs:774-789`), concurrent `/api/check` requests for one domain each start
their own, a prefetching browser can start runs, and the only limits are per-IP and global
(`crates/lens/src/security/rate_limit.rs:58-76`). Recording into the history runs on the request
path after the run, behind the store's write mutex (`crates/lens/src/check.rs:173`;
`v2-history` review, `specs/features/v2-history/report.md:179`, deferred to this feature).

## Proposed outcome

SDD Phase 2 criterion V2.6 (§4.4, S17–S19, S33, S8) holds in lens: the history is the cache (a
request is answered from the domain's latest entry when it was last seen within the cache window),
at most one fresh run per domain per minute starts, concurrent requests join the run in flight,
fresh runs need the client's hourly budget, the per-IP and global limits and a slot of the
domain-run limiter (S33's numbers as lens config values), a request carrying
`Sec-Purpose: prefetch`/`prerender` starts nothing, badge and OG read the history, an incomplete
run is never a cache hit, and recording runs off the request path.

## Affected users and systems

lens only (`crates/lens`, plus a small read in `crates/store` and `crates/engine`). Visitors of
`/api/check`, badge and OG card consumers. The SDD (`specs/sdd/v2.md` §4.4, Phase 2 table).
netray-ops gains config keys to deploy (out of scope here).

## Constraints

- Builds on the unmerged `v2-history` (V2.1–V2.5). Released only within `0.30.0` (operator,
  2026-10-17: 0.27.0–0.29.0 are not cut).
- lens's V1 wire and goldens stay byte-identical for fresh runs; the V1 wire is removed at the
  cutover (Phase 5), so anything that exists only to serve it is transitional.
- Config stays per service (repo `CLAUDE.md`); `netray.toml` does not exist before the cutover (S10).
- Rate limiters and the throttle stay in memory only (§4.7).

## Open questions

None. The three questions the spec raised (V1 hit body, badge and OG budget, `dkim_selectors`
and the throttle) were decided by the operator on 2026-10-17; see the spec's Decision log.
