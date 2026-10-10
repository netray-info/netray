# Spec: lens admission metrics

Status: Done
Created: 2026-10-10
Finished: 2026-10-10

## Goal

lens exports what V2's admission numbers need (V2 SDD S18, §4.4), from startup on: the outcome of every check request, the concurrency and duration of fresh runs, and an hourly distribution of fresh runs per client, without exporting any client key; and the badge and rate-limit counters exist at zero before their first event. Released as `0.23.1`, so the second baseline pull before V2 Phase 2 has weeks of data.

## Non-goals

- Traefik per-router series (argus-oci's part of the split).
- Renaming metrics into the `netray_` namespace (S11, at the cutover); the names stay `lens_*`.
- Metrics of the other services.

## Context and constraints

- The check path is `run_check_handler` (`crates/lens/src/routes.rs:1079`): domain validation, then `check_rate_limit` (`:1097`, lens's own per-IP and global limiters, `crates/lens/src/security/rate_limit.rs:57`), then the cache lookup (`:1105`), then a fresh `run_check_with_input` (`:1116`). The client key is `client_ip` from `state.ip_extractor` (`:1048`).
- Rate-limit rejections count `lens_rate_limit_hits_total{scope}` in `netray_common::rate_limit` (`crates/common/src/rate_limit.rs:45`, `:71`); lens's scopes are `per_ip`, `global` (`security/rate_limit.rs:65`, `:71`), `badge` (`routes.rs:912`) and `og` (`og/handler.rs:125`). Badge requests count `lens_badge_requests_total{cache}` with `hit`, `throttled`, `failed`, `miss` (`routes.rs:892-955`).
- The `metrics` exporter emits a series only after its first increment (argus-oci confirmed: no describe, no zero-init). The recorder is installed inside `netray_common::server::serve_metrics` (`crates/common/src/server.rs:49`), in a task lens spawns (`crates/lens/src/lib.rs:192`); a `PrometheusBuilder` without buckets renders a histogram as a summary.
- S27: nothing per visitor leaves the host; only aggregates are exported.

## Requirements

1. `lens_check_requests_total{result}` counts every check request that passed domain validation once, with `result` one of `rate_limited`, `cache_hit`, `fresh`.
2. Fresh runs are measured: `lens_runs_in_flight` (gauge) is incremented before `run_check_with_input` and decremented when it returns, also when the handler future is dropped mid-run; `lens_run_duration_seconds` (histogram, buckets `0.5 1 2 5 10 15 20 30`) observes each fresh run's wall time.
3. `lens_client_hourly_runs` (histogram, buckets `1 2 3 5 10 20 50 100`): lens keeps an in-memory count of fresh runs per client key; once an hour it observes one value per client that had at least one fresh run in that hour and clears the map. The client key is never exported, logged or persisted. Its help text says that a restart loses the partial hour.
4. At startup, after the recorder is installed, lens creates at zero every series of `lens_check_requests_total` (three results), `lens_badge_requests_total` (four cache values) and `lens_rate_limit_hits_total` (four scopes), and `lens_runs_in_flight` at 0.
5. `netray_common::server` lets a service install the recorder with per-metric histogram buckets and run code after the install, before serving; services that do not ask keep today's behaviour.
6. Every lens result, golden and existing metric stays as today.

## Phase 1 — Request, run and zero series

**Depends on:** none
**Requirements:** 1, 2, 4, 5, 6

### Test Scenarios

- GIVEN a check request for a valid domain with no cache entry WHEN handled THEN `lens_check_requests_total{result="fresh"}` is 1 and the other results 0.
- GIVEN the same domain requested again within the cache TTL WHEN handled THEN `result="cache_hit"` is 1.
- GIVEN a per-IP limit of 1 and two requests from one client WHEN handled THEN `result="rate_limited"` is 1.
- GIVEN an invalid domain WHEN handled THEN no `lens_check_requests_total` series is incremented.
- GIVEN a fresh run that a stub check holds open WHEN observed during the run THEN `lens_runs_in_flight` is 1, and 0 after it returns; GIVEN the handler future dropped mid-run THEN 0.
- GIVEN a fresh run WHEN it returns THEN `lens_run_duration_seconds` has one observation, rendered with the buckets of R2 (`le="0.5"` … `le="30"`), not as a summary.
- GIVEN lens started with no request served WHEN `/metrics` is rendered THEN it lists `lens_check_requests_total` for all three results, `lens_badge_requests_total` for all four cache values and `lens_rate_limit_hits_total` for all four scopes, each at 0, and `lens_runs_in_flight 0`.
- GIVEN a service that calls `serve_metrics` as today WHEN started THEN it renders as before (no buckets, no extra series).
- GIVEN the lens goldens and results tables WHEN run THEN unchanged.

## Phase 2 — Hourly runs per client

**Depends on:** 1
**Requirements:** 3

### Test Scenarios

- GIVEN client A with three fresh runs and client B with one in the current hour WHEN the hour is flushed THEN `lens_client_hourly_runs` has count 2 and sum 4, rendered with the buckets of R3.
- GIVEN a flush WHEN the next hour has no fresh run THEN the following flush observes nothing (the map was cleared).
- GIVEN cache hits and rate-limited requests from a client WHEN flushed THEN they are not counted.
- GIVEN a client made fresh runs WHEN `/metrics` is rendered after a flush THEN it does not contain the client's address, and the per-client counter writes no log line naming it (lens's request span already records `client_ip`, `routes.rs:1088`; that is unchanged).
- GIVEN the help text of `lens_client_hourly_runs` WHEN rendered THEN it says a restart loses the partial hour.

## Decision log

- Start this slice before `v2-modules`, released as `0.23.1` on the operator's go (operator, 2026-10-10, via the planning session).
- lens owns these metrics, argus-oci Traefik's per-router series (operator, 2026-10-10).
- `rate_limited` is counted in lens's check handler, where lens checks its limits (`routes.rs:1097`), over the common rate-limit helper the first plan named: the helper is shared by the badge and OG limiters, which are not check requests.
- An invalid domain is not counted, over a fourth `invalid` result: the agreed metric has three results, and invalid input is no admission load.
- Buckets as proposed by the planning session (R3) and fresh-run buckets up to the 20 s hard deadline plus 30 (R2).
- The hour boundary is process-local, over persisting the map: no client key leaves memory (S27).

- The address check covers this feature's metric and counter, over all of lens's log output: the request span has logged `client_ip` since before this feature (`routes.rs:1088`), which this slice does not change (phase 2 amendment, 2026-10-10).

## Open decisions

None.

## Out of scope

- Admission control itself (V2 Phase 2, S18).
