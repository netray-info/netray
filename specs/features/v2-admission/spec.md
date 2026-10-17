# Spec: v2-admission

Status: Ready for Implementation
Created: 2026-10-17

## Goal

lens answers every request for a domain from the history when the domain's latest entry was last
seen within the cache window, and otherwise starts at most one fresh run per domain per minute,
which concurrent requests join and which needs a free slot of 8, the client's hourly budget of 30
and today's per-IP and global limits. A request carrying `Sec-Purpose: prefetch` or `prerender`
starts nothing. Badge and OG read the history. An incomplete run is never a cache hit. Every run
executes in its own task and is recorded into the history after its result is published, so no
V1, badge or OG response waits on the history store. The moka cache no longer decides
freshness. Fresh-run responses on lens's V1 wire, and its goldens, are unchanged. The admission
path is one public interface, `check_limits` and `admit` returning a run stream or a refusal with
its reason and retry time, which the Phase 3 and Phase 4 features (`v2-api`, `v2-result-page`)
call. This is SDD Phase 2 criterion V2.6 (§4.4; S17–S19, S33, S8). It also moves V2.5's "never
served as a cache hit" onto the history, and it takes the history record off the request path
(deferred by the `v2-history` review, `specs/features/v2-history/report.md:179`).

## Non-goals

- `/r/<id>` for history entries, its `410`, the privacy page (V2.7–V2.8, feature `v2-entries`).
- API v1 and its `429` endpoint (`POST /domains/{domain}/runs`, `v2-api` Phase 3). This feature
  provides the `admit` it calls.
- The result page, "re-check now", S26's refusal line and the streaming island (`v2-result-page`,
  `v2-pages`). lens's V1 wire has no re-check; the "re-check bypasses the window, never the
  throttle" rule holds here because `admit` never reads the window (requirement 20).
- Moving the admission keys into `netray.toml` (S10, cutover).
- Re-setting the admission numbers from measurement (S33: after V2).
- Streaming lens's V1 SSE section by section. The V1 SSE response stays built after the run ends
  (`crates/lens/src/routes.rs:1336-1373`, `:1396-1407`), and joiners get the same events. The run
  stream of requirement 21 carries live section events for the Phase 4 consumers.
- A release. This ships inside `0.30.0` (operator decision 2026-10-17, Decision log). This spec
  amends nothing in S37; `v2-cutover` carries that amendment.

## Context and constraints

- Builds on the unmerged `v2-history` worktree. Paths and lines below are read there.
- **Build order** (one order in every spec): lane A `v2-history` → `v2-admission` → `v2-entries` →
  `v2-api`; lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C, after the operator's UI
  review at midday 2026-10-18, `v2-inspectors` (its Phase 2 before `v2-result-page` Phase 1), then
  `v2-result-page`; then `v2-cutover` (operator decision 2026-10-17, Decision log). This feature
  starts when `v2-history` has merged.
- **Cache today.** moka, built when `[cache] enabled` with TTL `ttl_seconds` (300)
  (`crates/lens/src/state.rs:87-93`; `CacheConfig` `crates/lens/src/config.rs:153-157`, default
  `:379-381`). `/api/check` reads it only without `dkim_selectors` (`routes.rs:759-772`). The badge
  checks freshness with `badges.ttl_seconds` (3600, `routes.rs:540-564`), and OG with
  `OG_TTL_SECONDS` = 3600 (`crates/lens/src/og/layout.rs:42`, `og/handler.rs:95-121`). moka's
  300 s TTL evicts first, so the effective window is 300 s everywhere. The cache writers
  (`crates/lens/src/cache.rs:37-49` `store_result`, `:54-92` `get_or_compute`) refuse incomplete
  results: that is V2.5's "never served as a cache hit" today.
- **V1 hit body.** A hit answers from `CachedResult` (sections with `BackendResult`, `OverallScore`,
  snapshot id; `cache.rs:13-22`). It sets `X-Cache: HIT` and `cached: true`
  (`routes.rs:1375-1393`, `:1469-1493`). A stored `Run` (`crates/lens/src/history.rs:50-99`) has
  check statuses, blocks, grade and section kinds. It lacks the V1 headlines, check messages,
  `BackendExtra` fields (`crates/lens/src/modules.rs:18-48`) and percentages
  (`routes.rs:955-1247`). So the V1 body cannot be rebuilt from the history; moka holds it until
  the cutover (Decision log).
- **One run per domain.** `/api/check` does not merge concurrent requests (`routes.rs:774-789`).
  Badge and OG coalesce only through moka's `and_compute_with` (`cache.rs:54-92`), and only
  between themselves. The badge's per-domain recompute limiter (quota period
  `badges.ttl_seconds`, burst 1, `state.rs:82-85`) runs before coalescing (`routes.rs:566-580`),
  so a concurrent second badge request gets `?`
  (`crates/lens/tests/sdd_lens_badges_p1_c11_cache_coalesce.rs:198-254`).
- **Recording.** `run_check_with_evidence` calls `history::record` after scoring
  (`crates/lens/src/check.rs:173`), awaited on the request path. `record` skips runs with
  `dkim_selectors` (`history.rs:112`). It takes the store's write mutex
  (`crates/store/src/lib.rs:164`) and logs and counts failures (`history.rs:118-126`). Badge
  and OG tests inject `badge_check_fn` (`state.rs:29-30`, `routes.rs:37-43`,
  `og/handler.rs:50-56`), which bypasses `run_check_with_evidence` and so is never recorded.
- **Section events.** `netray_engine::run` sends each `netray_engine::SectionEvent` (`protocol`,
  `outcome`; `crates/engine/src/lib.rs:217-220`) on its channel the moment the section finishes
  (`:326`). Its `SectionOutcome` (`:117-130`) is `Measured { checks, evidence }` (each check a
  `netray_model::CheckResult` with `id`, `status`, `findings` and `evidence` block references, and
  the section's `Block`s), `NotApplicable { reason }`, `Incomplete { reason }` or `TimedOut`, so one
  event holds everything a section fragment renders. `run_check_with_evidence` collects them and
  adapts them only after the run (`check.rs:111-135`). `history::build_run` builds the stored `Run`
  (`history.rs:50`).
- **Sibling consumers.** Both reach this path through one trait, `netray_web::api::DomainRuns`,
  that `v2-api` defines (its requirement 1) and lens implements: `start(domain, client, trigger,
  prefetch)` calls `check_limits` then `admit` (requirement 20). `v2-api` requirement 14 posts runs
  through it (joins, throttle, admission, one record, the window bypassed) with trigger `api` and
  answers `{recorded, entry_id, run, diff}` with one code per refusal reason. `v2-result-page`
  requirement 19 calls it with a trigger `visit` or `recheck` and streams finished then live
  sections; its requirement 21 words each refusal from its reason, the seconds since the last run
  and whether it was incomplete; and its Decision log seeds the screenshots with a client budget
  of 0 (built by its requirement 28).
- **Store reads.** `HistoryStore::latest` parses the head's full run JSON (`store/src/lib.rs:208-211`,
  `:273-282`); `head_in` reads id, `last_seen_at` and `predecessor_id` only, privately (`:284-304`).
  `record` returns `First`, `Identical`, `Changed` with the entry id, or `NotStored` (`:89-100`).
  `netray_engine::history::rescore` gives a stored run's grade under a profile, not its percentage
  (`crates/engine/src/history.rs:13-48`); `Score` has `overall_percentage`
  (`crates/engine/src/scoring.rs:23-36`). OG renders grade, percentage and a timestamp
  (`crates/lens/src/og/render.rs:98-124`).
- **Limits today.** Per-IP 10/min burst 3 and global 100/min burst 20 are `[rate_limit]` keys
  (`config.rs:162-171`). Their defaults are already S33's numbers (`:383-397`), and so is the
  production fixture (`crates/lens/tests/fixtures/lens.production.toml:40-44`). They are checked
  before the cache lookup (`routes.rs:752-756`, `crates/lens/src/security/rate_limit.rs:58-76`), so
  cache hits spend them. Badge and OG are outside them. Refusals are `AppError::RateLimited`: 429
  `RATE_LIMITED` with `Retry-After` (`crates/lens/src/error.rs:22`, `:53`, `:67`, `:77`).
- **Metrics today.** `lens_check_requests_total{result}` has fresh, cache_hit and rate_limited.
  Other series: `lens_badge_requests_total{cache}` (hit, miss, throttled, failed),
  `lens_rate_limit_hits_total{scope}`, `lens_runs_in_flight` and
  `lens_history_record_errors_total`, all registered at zero (`crates/lens/src/metrics.rs:17-35`).
  `RunGuard` and `observe_run` wrap `/api/check` fresh runs only (`routes.rs:774-784`). The gate
  script checks the zero series (`tests/repo/test_lens_admission_metrics.sh:21-34`).
- **Config rules.** Config stays per service, and `netray.toml` arrives with the cutover (repo
  `CLAUDE.md`, SDD S10). Every config struct has `deny_unknown_fields`
  (`tests/repo/test_config_strict.sh`). A startup `.expect` on a config value needs a `validate()`
  rule (`config.rs:494-505`) and a `startup_rejects` row (`tests/repo/test_check_config.sh:83`).
- **SDD.** §4.4 (`specs/sdd/v2.md:580-602`), S8 (`:34`), S17–S19 (`:42-44`), S33 (`:60`), §9.1's
  incomplete-run rule for badge and OG (`:977-981`), §9.2's `netray_admission_rejections_total{reason}`
  (`:1000`) and `netray_runs_total{trigger}` (`:992`, visit, recheck, api), and V2.6 (`:1085`). Rate limiters and the throttle stay in memory only (§4.7, `:624-631`).
- **Tests whose meaning changes.** Each change needs `ADLC-Test-Change`:
  - Badge coalescing and the warm-cache test: `sdd_lens_badges_p1_c11_cache_coalesce.rs:146`, `:198`.
  - `admission_metrics.rs:187` (`c7` cache hit) and `:260` (gauge after the handler drops).
  - `incomplete_results.rs:219`, `:324`, `:344`; `history_badge.rs:125-149`.
  - The `cache_built_when_enabled` test in `state.rs:262-268`.
  - Tests that run one domain twice within a minute without a history store; they now meet the
    throttle.
- Gate: `just adlc-verify`; full suite `just check`; `--workspace` for feature-sensitive tests.
  Amend `specs/sdd/v2.md` in the same commit as the code that changes what it says.

## Requirements

1. lens config gains `[admission]` with `deny_unknown_fields`. It has two keys:
   `client_budget_per_hour`, default 30, and `domain_run_slots`, default 8. The numbers are S33's;
   the names are (mandate default, see Decision log; `client_budget_per_hour` is the name
   `v2-result-page` assumes, set by `LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR`). `client_budget_per_hour = 0` is
   valid and refuses every budgeted run (requirement 13). `validate()` refuses
   `domain_run_slots = 0` and names the key. `lens.production.toml` gains the table with both
   values. `test_check_config.sh` gets one `startup_rejects` row for `domain_run_slots = 0`.
   `[rate_limit]` keeps its keys and defaults, which are S33's per-IP and global numbers.
   `[cache]` keeps `enabled` and `ttl_seconds`; `ttl_seconds` is now §4.4's cache window, read by
   callers from `state.config.cache.ttl_seconds`.
2. Every run without request options, whichever route or caller of `admit` started it, goes
   through one per-domain coordinator keyed by `cache_key`. A request for a domain whose run is in
   flight joins that run and gets its result: the same `CheckOutput`, facts blocks and snapshot id.
   It starts no run of its own. A run with `dkim_selectors` never joins a run and is never joined.
3. A run executes in a spawned task that keeps running when every requester has gone. The task
   runs the check and forwards each section event as it arrives (requirement 21). When the run
   is complete it creates the snapshot, as `routes.rs:797-801` does today. It builds the `Run`
   once (`history::build_run`) and then publishes the result to every waiter. Only then does it
   record the run in the history, through `history::record` moved out of `check.rs:173` (recorded
   or diffed once per run, never with `dkim_selectors`, as `v2-history` R13 says), and publishes
   what the store reported. Last, it removes the domain's in-flight registration. A request
   arriving between publish and removal joins and gets the published result. No V1, badge or OG
   response waits on the history store; only a consumer that reads the `Recorded` event of
   requirement 21 (`v2-api`'s and `v2-result-page`'s run answers) waits for it.
4. Runs injected through `badge_check_fn` go through the same task and are recorded, with no facts
   blocks.
5. A request that joins counts `lens_check_requests_total{result="joined"}`. That series is
   registered at zero and `test_lens_admission_metrics.sh` checks it. `fresh` counts only requests
   that started a run. For runs started by `/api/check`, the task holds `RunGuard` and
   `observe_run`, so `lens_runs_in_flight` stays up until the run ends, even when the requester has
   gone.
6. A cache hit is a domain whose head entry was last seen less than `cache.ttl_seconds` ago
   (`now - last_seen_at < ttl_seconds`, Unix seconds). This holds for `/api/check` without options,
   for the badge and for OG alike. With `[cache] enabled = false`, or without a history store,
   nothing is a cache hit. `badges.ttl_seconds` keeps only the recompute limiter's period.
   `OG_TTL_SECONDS` goes.
7. `netray-store` gains `head(domain)`. It returns the head's id, `last_seen_at`, grade and profile
   version and reads no payload column, as `head_in` does.
8. A V1 `/api/check` hit is answered from moka, which now holds only the V1 rendering
   (`CachedResult`) of the run that last recorded the head. The holder is keyed by the entry id
   that `record` returned (`First`, `Identical` or `Changed`), and its TTL is the cache window.
   An `Identical` record replaces the value under the same id with the newest run's rendering. A
   hit needs requirement 6 and a value held under the head's id; a value held under any other id
   of the domain is never read. It answers as today: `X-Cache: HIT`, `cached: true` and the held
   snapshot id. `NotStored` and a failed record hold nothing.
   `cache::store_result` and `cache::get_or_compute` go, and badge and OG never read moka.
   SDD §4.4 records the holder as V1-only, removed with the V1 wire at the cutover.
9. A run that ended incomplete is never served as a cache hit by any route. The head entry it did
   not replace stays the only cache candidate. A complete run that recorded is a hit for the next
   request within the window. V2.5's "never served as a cache hit" holds through the history.
10. The badge renders a hit from the entry's grade. Its Cache-Control is long only when the shown
    entry is fresh (requirement 6) and short otherwise. After the request's own or joined run:
    - complete: that run's grade, with the long Cache-Control;
    - incomplete, with a head entry: that entry's grade, with the short Cache-Control (SDD §9.1);
    - incomplete, without one: unscored, with the short Cache-Control (`v2-history` R14).
11. OG renders a hit from the entry: its grade, its `last_seen_at` as the timestamp, and its
    percentage. The percentage is the stored run scored by `netray_engine` under the profile of
    the entry's version, and the configured profile when the versions are equal. A new function
    in `netray_engine::history` returns the `Score`; `rescore` becomes its grade. After an
    incomplete own or joined run, OG renders the head entry if one exists and today's unscored
    card otherwise. Cache-Control follows requirement 10.
12. At most one run per domain (`cache_key`) starts per 60 s (SDD §4.4, a constant). The minute is
    counted from run start, whatever route, trigger or option started the run. It is measured on
    tokio's clock and held in memory, pruned of expired domains. Each throttle record holds the
    run's start and, once the run ended, whether it was complete. A throttle refusal carries
    `retry_after_s` = the seconds left in the minute, `last_run_ago_s` = the seconds since that run
    started, and `last_incomplete` = true only when that run ended incomplete (false while it is
    in flight or when it ended complete). When throttled:
    - `/api/check` answers 429 `RATE_LIMITED` with `Retry-After` = `retry_after_s` (§4.4: the API
      answers 429);
    - badge and OG render the head entry with the short Cache-Control, or `?` as
      `routes.rs:566-580` does, counted `lens_badge_requests_total{cache="throttled"}`.
13. A fresh run with trigger `visit`, `recheck` or `api` (requirement 20) spends one token of a
    per-client GCRA limiter. The limiter is keyed by the client address the per-IP limiter uses,
    with quota `client_budget_per_hour` per hour and burst equal to that quota (mandate default,
    see Decision log). With `client_budget_per_hour = 0` no limiter is built and every such run is
    refused `budget` with `retry_after_s` = 3600 (spec drafter, Decision log). Cache hits and joins
    spend none. A refusal on `/api/check` answers 429 `RATE_LIMITED` with `Retry-After` =
    `retry_after_s`. Runs with trigger `badge` or `og` never spend a token.
14. Every fresh run holds one permit of a `domain_run_slots` semaphore for its whole run, taken
    without waiting. Joins take none. With no permit free the refusal is `slots` with
    `retry_after_s` = 1 (mandate default, see Decision log); `/api/check` answers 429 `RATE_LIMITED` with
    `Retry-After: 1`. Badge and OG answer as in requirement 12.
15. Admission order for `/api/check` without options: validate, `check_limits` (unchanged
    position, so hits keep spending them), cache hit, then `admit`: join, prefetch rule
    (requirement 18), throttle, slot, budget, start. Badge and OG: validate, cache hit, then
    `admit`: join, prefetch rule, the badge recompute limiter (unchanged, applied by `admit` for
    the triggers `badge` and `og` only), throttle, slot, start. A refused request spends no budget
    token, holds no permit and does not start a throttle minute.
16. A run with `dkim_selectors` is never a cache hit and never joins (requirement 2). It needs the
    per-IP and global limits, the throttle, a slot and a budget token, and it starts the domain's
    throttle minute: it is bound by a throttle minute any run started, and a run without options
    is bound by the minute it started.
17. `lens_admission_rejections_total{reason}` counts every refusal of `check_limits` and `admit`,
    with `reason` one of `throttle`, `budget`, `limit` (the refusal reasons `ip_limit` and
    `global_limit`) and `slots`. The name is (mandate default, see Decision log) after SDD §9.2. All four
    series are registered at zero, and `test_lens_admission_metrics.sh` checks them. A refused
    `/api/check` request also counts `lens_check_requests_total{result="rate_limited"}`.
18. A request to `/api/check`, the badge or OG whose `Sec-Purpose` value contains the token
    `prefetch` or `prerender` never starts a run. One public helper,
    `netray_common::prefetch::is_prefetch(&HeaderMap)`, decides it for lens and netray-web alike: tokens are separated by `;` or `,` and compared case insensitively, so a
    combined value such as `prefetch;prerender`, which a prerender request may send, is caught. `admit` takes its result and, for such a request, joins the run in
    flight or answers `NotStarted`, never `Admitted` with a new run and never `Refused`. On
    `NotStarted`, `/api/check` answers 503 with error code `PREFETCH_REFUSED` in the
    architecture-rules error format and `Cache-Control: no-store` (mandate default, see Decision
    log); badge
    and OG render the head entry with the short Cache-Control, or `?`. The request spends no budget
    token and starts no throttle minute. A request without the header, or with another value (for
    example `Sec-Purpose: none`), is unaffected.
19. SDD amendments, each in the commit of the phase that builds it:
    - §4.4 "history is the cache": the V1 holder of requirement 8.
    - §4.4 "Admission": the lens keys `[admission]` and `[rate_limit]` until `netray.toml` (S10);
      the order of requirement 15; the per-route refusal answers; the badge and OG budget rule;
      the `dkim_selectors` rule; a budget of 0.
    - §4.4 "Prerender": the token rule and the answers of requirement 18.
    - S33: "`netray.toml` values" becomes "lens config values until the cutover, then
      `netray.toml`". Written in Phase 1, which introduces `[admission]`.
    - Phase 2 table: V2.6 reads "with S33's numbers" in place of "numbers from the baseline (the
      §4.4 TODO resolved)", and the table marks V2.6 as built by `v2-admission`.
    - §9.2: `netray_runs_total`'s `trigger` reads (visit, recheck, api, badge, og), the values of
      requirement 20's `Trigger` (M1, Decision log).
20. The admission path is public in `crates/lens/src/admission.rs`, and `/api/check`, badge and OG
    use it as any other caller would:
    - `check_limits(&state, client: IpAddr) -> Result<(), Refusal>` applies the per-IP and global
      limiters. `/api/check` calls it before the cache hit; a caller that needs no cache step
      (lens's `DomainRuns::start`, which serves `v2-api`'s and `v2-result-page`'s run POSTs)
      calls it before `admit`. Badge and OG never call it.
    - `admit(&state, domain, client: IpAddr, trigger, prefetch: bool) -> Admit` runs the steps
      of requirement 15 after the cache hit, for runs without options.
      `Admit` is `Admitted(RunStream)` (a started or joined run), `Refused(Refusal)`, or
      `NotStarted` (requirement 18).
    - `Trigger` is `visit | recheck | api | badge | og`. `visit`, `recheck` and `api` are admitted
      identically and spend the budget; `badge` and `og` spend none. `api` is `v2-api`'s run
      POST. Every `Trigger` value is a value of SDD §9.2's `netray_runs_total{trigger}`, which
      `v2-cutover` counts; `badge` and `og` join visit, recheck and api there (M1, requirement 19).
      `admit` never reads the cache window:
      the window is the caller's step before `admit`, and a re-check skips it, so a re-check
      bypasses the window and never the throttle (§4.4). `/api/check` passes `visit`.
    - `Refusal` is `{reason: throttle | budget | ip_limit | global_limit | slots, retry_after_s:
      u64, last_run_ago_s: Option<u64>, last_incomplete: bool}`. `last_run_ago_s` is set only for
      `throttle`, and `last_incomplete` is false for every other reason.
    - The `dkim_selectors` path of `/api/check` is internal; no other caller runs with options.
21. A `RunStream` yields, in order: one `Section(netray_engine::SectionEvent)` per finished
    section, the engine's event unchanged, carrying the section's checks (with findings and
    evidence references) and blocks or its not-applicable, incomplete or timed-out outcome (a
    joiner first receives every section already finished, then the live ones), one `Done` carrying the
    published result (`CheckOutput`, facts blocks, snapshot id, and the `Run` of requirement 3),
    and one `Recorded(Option<Recorded>)` carrying what `HistoryStore::record` returned, or none
    without a store or after a failed write. Then it ends. `Recorded` never precedes `Done`.
    Dropping a `RunStream` never cancels the run. lens's V1 routes stop reading at `Done`.

## Phases

## Phase 1 — Admission config

**Depends on:** `v2-history` merged (lane A)
**Requirements:** 1

`crates/lens/src/config.rs`: `AdmissionConfig`, defaults and `validate()` rules. Config literals in
the tests gain the field. Rows go into `lens.production.toml` and `tests/repo/test_check_config.sh`.
Nothing reads the values yet.

### Test Scenarios

GIVEN a config without `[admission]` WHEN it loads THEN `client_budget_per_hour` is 30 and `domain_run_slots` is 8.
GIVEN `[admission] domain_run_slots = 0` WHEN it loads THEN it is refused with a message naming `admission.domain_run_slots`.
GIVEN `[admission] client_budget_per_hour = 0` WHEN it loads THEN it is accepted; GIVEN `client_budget_per_hour = 1` or `domain_run_slots = 1` THEN it is accepted.
GIVEN `LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR=0` WHEN the config loads THEN `client_budget_per_hour` is 0.
GIVEN an unknown key under `[admission]` WHEN the config loads THEN it is rejected.
GIVEN `lens.production.toml` WHEN `netray lens --check-config` runs THEN it exits 0; GIVEN the `domain_run_slots = 0` `startup_rejects` row applied THEN it exits 1; GIVEN `client_budget_per_hour = 0` applied THEN it exits 0.
GIVEN a config without `[rate_limit]` WHEN it loads THEN the limits are 10/3/100/20, unchanged.

## Phase 2 — One run per domain, recorded off the request path

**Depends on:** `v2-history` merged (lane A)
**Requirements:** 2, 3, 4, 5, 21

A new `crates/lens/src/admission.rs` holds the in-flight map, the run task (run, forward section
events, snapshot, build `Run`, publish, record, publish the record, deregister), `RunStream`,
`Trigger`, and `admit`, which in this phase only starts or joins (`Admit::Admitted`).
`run_check_handler`, `badge_handler` and `og_handler` start or join runs through `admit`. The
`history::record` call moves from `check.rs` into the task. moka still decides hits in this
phase.

### Test Scenarios

GIVEN a gated run of `example.com` started by `/api/check` WHEN a second `/api/check` for `example.com` arrives before the gate opens THEN exactly one run executes, both responses carry the same section payloads and snapshot id, and `joined` is 1, `fresh` 1.
GIVEN the same gated run WHEN a badge request for `example.com` arrives THEN it joins: one run executes.
GIVEN a gated run of `example.com` WHEN `/api/check` for `example.net` arrives THEN a second run executes (another domain never joins).
GIVEN a gated run of `example.com` without options WHEN a request with `dkim_selectors` arrives THEN a second run executes, and a request without options arriving during the `dkim_selectors` run does not join it.
GIVEN a gated run whose requester's future is dropped WHEN the gate opens THEN the run completes and `history.db` holds its entry; `lens_runs_in_flight` is 1 until the run ends, then 0.
GIVEN a history store whose write mutex is held by the test WHEN a complete run finishes THEN the response arrives before the mutex is released.
GIVEN a published result whose record is still pending WHEN a request for the domain arrives THEN it gets the published result and no second run executes.
GIVEN a `badge_check_fn` mock returning a complete run WHEN the badge route runs it THEN `history.db` holds one entry.
GIVEN a run with `dkim_selectors` WHEN it completes THEN `history.db` holds no new entry (R13 of `v2-history` unchanged).
GIVEN two sequential `/api/check` requests with no run in flight between them WHEN both complete THEN `fresh` is 2 and `joined` 0.
GIVEN a run with five sections whose DNS and TLS sections have finished WHEN a caller joins through `admit` THEN its `RunStream` yields the DNS and TLS `Section` events first, then the other three as they finish, then `Done`, then `Recorded`.
GIVEN a started run WHEN its `RunStream` is read to the end THEN it yields exactly one `Section` per configured section, one `Done` and one `Recorded`, in that order.
GIVEN a history store whose write mutex is held by the test WHEN a complete run finishes THEN the `RunStream` yields `Done` before the mutex is released and `Recorded(Some(First))` after it.
GIVEN a run that ends incomplete WHEN its `RunStream` ends THEN `Recorded` carries `NotStored`; GIVEN no history store THEN it carries none; GIVEN a store whose writes fail THEN it carries none.
GIVEN a `Done` from `admit` with trigger `recheck` WHEN compared with the `Run` the store recorded THEN they are equal.

## Phase 3 — The history is the cache

**Depends on:** Phase 2
**Requirements:** 6, 7, 8, 9, 10, 11

`crates/store`: `head`. `crates/engine/src/history.rs`: score a stored run. `crates/lens`: hit
decision from the head, moka narrowed to the V1 holder, badge and OG read the history, and
`cache.rs`'s writers and `OG_TTL_SECONDS` removed. SDD §4.4 cache paragraph.

### Test Scenarios

GIVEN a head entry for `example.com` last seen 10 s ago with its V1 rendering held WHEN `/api/check` arrives THEN it is answered without a run, with `X-Cache: HIT`, `cached: true` and the held snapshot id.
GIVEN a head entry last seen 301 s ago (window 300) WHEN `/api/check` arrives THEN a run starts.
GIVEN a fresh head entry but no held rendering (after a restart) WHEN `/api/check` arrives THEN a run starts.
GIVEN a rendering held under the domain's previous entry id and a newer head entry (recorded by a run whose rendering was not held) WHEN `/api/check` arrives THEN a run starts and the previous rendering is not served.
GIVEN `[cache] enabled = false` and a fresh head entry WHEN `/api/check` arrives THEN a run starts.
GIVEN no history store WHEN `/api/check` arrives twice 61 s apart (tokio clock) THEN both start runs.
GIVEN a complete run that recorded `Identical` WHEN `/api/check` arrives within the window THEN it is a hit carrying the newest run's rendering.
GIVEN a fresh head entry and a following incomplete run WHEN the next `/api/check` arrives THEN it is not served the incomplete run; when the head is still fresh it is the head's hit.
GIVEN an incomplete run and no entry WHEN any route asks within the window THEN no route answers from that run.
GIVEN a store whose writes fail WHEN a complete run finishes THEN the next request is no hit.
GIVEN a head entry with grade B last seen 10 s ago WHEN the badge is requested THEN it shows B with the long Cache-Control and no run starts.
GIVEN a head entry with grade B last seen 2 h ago and the recompute limiter exhausted WHEN the badge is requested THEN it shows B with the short Cache-Control.
GIVEN a head entry with grade B and a fresh run that ends incomplete with a hard fail WHEN the badge is requested THEN it shows B with the short Cache-Control.
GIVEN no entry and a fresh run that ends incomplete WHEN the badge is requested THEN it is unscored with the short Cache-Control.
GIVEN no entry and a fresh complete run with grade F WHEN the badge is requested THEN it shows F with the long Cache-Control.
GIVEN a head entry last seen 10 s ago WHEN OG is requested THEN the card shows the entry's grade, its percentage under its profile, and `last_seen_at`, and no run starts.
GIVEN a stored run WHEN scored under its own profile THEN the grade equals the entry's grade and `rescore` returns that grade.
GIVEN `HistoryStore::head` on a domain with three chained entries THEN it returns the chain head's id, `last_seen_at`, grade and profile version; on an unknown domain it returns none.

## Phase 4 — Throttle, budget, slots

**Depends on:** Phase 1, Phase 3
**Requirements:** 12, 13, 14, 15, 16, 17, 20

`crates/lens/src/admission.rs`: the throttle on tokio's clock with each run's outcome, the
per-client limiter, the slot semaphore in `AppState`, `check_limits`, `Refusal` and
`Admit::Refused`, the order of requirement 15, the refusal answers per route, and the rejection
counter. SDD §4.4 admission paragraph, the Phase 2 table and §9.2's trigger values. Requirement 20 closes in
Phase 5 with the prefetch argument.

### Test Scenarios

GIVEN a run of `example.com` that ended incomplete 10 s ago (tokio clock paused) WHEN `/api/check` arrives THEN it answers 429 with `Retry-After: 50` and `lens_admission_rejections_total{reason="throttle"}` is 1.
GIVEN the same WHEN the clock advances to 60 s after the run started THEN `/api/check` starts a run.
GIVEN the same at 59 s THEN it still answers 429.
GIVEN a run started by the badge 10 s ago WHEN `/api/check` for the domain arrives THEN it is throttled.
GIVEN a throttled domain with a head entry graded C WHEN the badge is requested THEN it shows C with the short Cache-Control and counts `cache="throttled"`; GIVEN no head entry THEN it shows `?`.
GIVEN a throttled domain WHEN `/api/check` for another domain arrives THEN it starts a run.
GIVEN `client_budget_per_hour = 2` WHEN one client starts runs of three different domains THEN the third answers 429 with a `Retry-After`, and `reason="budget"` is 1.
GIVEN `client_budget_per_hour = 1` and a fresh head entry for `example.com` WHEN the client asks for `example.com` twice and then `example.net` THEN both `example.com` answers are hits and `example.net` starts a run (hits spend no budget).
GIVEN `client_budget_per_hour = 1` and a gated run of `example.com` started by client X WHEN client Y requests `example.com` and then `example.net` THEN Y's first request joins and Y's `example.net` starts a run (a join spends no budget).
GIVEN `client_budget_per_hour = 1` WHEN two clients each start one run THEN both run.
GIVEN `client_budget_per_hour = 1` WHEN the badge of three domains is requested from one address THEN all three start runs (badge spends no budget).
GIVEN `domain_run_slots = 1` and a gated run of `example.com` WHEN `/api/check` for `example.net` arrives THEN it answers 429 with `Retry-After: 1` and `reason="slots"` is 1; WHEN the badge for `example.net` is requested THEN it shows `?`.
GIVEN `domain_run_slots = 1` and a gated run of `example.com` WHEN a request for `example.com` arrives THEN it joins (a join needs no slot).
GIVEN `domain_run_slots = 1` and a refused `example.net` WHEN the gate opens and `example.net` is asked again THEN it starts a run without being throttled and its client's budget is unchanged by the refusal.
GIVEN `per_ip_per_minute = 1, per_ip_burst = 1` and a fresh head entry WHEN one client asks twice THEN the second answers 429 and `reason="limit"` is 1 (hits keep spending the per-IP limit).
GIVEN a run with `dkim_selectors` of `example.com` 10 s ago WHEN `/api/check` with `dkim_selectors` for the domain arrives THEN it answers 429; WHEN `/api/check` without options arrives THEN it is throttled too.
GIVEN lens started WHEN `/metrics` is read before any request THEN the four `lens_admission_rejections_total` series and `lens_check_requests_total{result="joined"}` render at 0.
GIVEN a run of `example.com` that ended incomplete 10 s after starting, 40 s ago (tokio clock) WHEN `admit` is called with trigger `recheck` THEN it answers `Refused{reason: throttle, retry_after_s: 10, last_run_ago_s: Some(50), last_incomplete: true}`.
GIVEN the same run ended complete THEN the refusal carries `last_incomplete: false`; GIVEN a `dkim_selectors` run of the domain still in flight THEN a run without options is refused `throttle` with `last_incomplete: false`.
GIVEN a head entry last seen 10 s ago and no throttle record WHEN `admit` is called with trigger `recheck` THEN it starts a run (it never reads the window); GIVEN trigger `visit` THEN it starts a run too.
GIVEN `client_budget_per_hour = 0` WHEN `admit` is called with trigger `visit` or `recheck` THEN it answers `Refused{reason: budget, retry_after_s: 3600, last_run_ago_s: None, last_incomplete: false}`; WHEN `/api/check` arrives with no entry THEN it answers 429 with `Retry-After: 3600`.
GIVEN `client_budget_per_hour = 0` WHEN the badge of a domain without an entry is requested THEN a run starts (badge spends no budget); WHEN `/api/check` for a domain with a fresh head entry and a held rendering arrives THEN it is a hit.
GIVEN `per_ip_per_minute = 1, per_ip_burst = 1` WHEN `check_limits` is called twice for one address THEN the second answers `Refused{reason: ip_limit}` with a `retry_after_s`; GIVEN the global limit exhausted THEN it answers `global_limit`; both count `reason="limit"`.
GIVEN `domain_run_slots = 1` and a gated run WHEN `admit` is called for another domain THEN it answers `Refused{reason: slots, retry_after_s: 1, last_run_ago_s: None, last_incomplete: false}`.
GIVEN the badge per-domain recompute limiter exhausted and a run of the domain in flight WHEN the badge is requested THEN it joins the run (the join precedes the recompute limiter).

## Phase 5 — Prefetch starts nothing

**Depends on:** Phase 4
**Requirements:** 18, 19, 20

`crates/common`: `prefetch::is_prefetch`. `crates/lens`: the `prefetch` argument of `admit` and `Admit::NotStarted`, so the
rule runs after the cache hit and join steps in every caller. The `PREFETCH_REFUSED` error
variant. SDD §4.4 prerender line. Requirement 19 closes with this
phase: its other items are written by the phases that build them (Phase 1: S33; Phase 3: the cache
paragraph; Phase 4: admission, the Phase 2 table, §9.2's triggers). This phase adds the
prerender item and checks that all of them are present.

### Test Scenarios

GIVEN no entry and no run in flight WHEN `/api/check/example.com` arrives with `Sec-Purpose: prefetch` THEN it answers 503 `PREFETCH_REFUSED` with `Cache-Control: no-store`, no run starts, and a following request without the header is not throttled.
GIVEN the same with `Sec-Purpose: prefetch;prerender` THEN the same.
GIVEN the same with `Sec-Purpose: PREFETCH` THEN the same.
GIVEN a fresh head entry with its rendering held WHEN `/api/check` arrives with `Sec-Purpose: prefetch` THEN it is a hit.
GIVEN a gated run in flight WHEN a request with `Sec-Purpose: prerender` arrives THEN it joins.
GIVEN a stale head entry graded A WHEN the badge is requested with `Sec-Purpose: prefetch` THEN it shows A with the short Cache-Control and no run starts; GIVEN no entry THEN it shows `?`.
GIVEN a stale head entry WHEN OG is requested with `Sec-Purpose: prefetch` THEN it renders the entry and no run starts.
GIVEN `Sec-Purpose: none` or no `Sec-Purpose` header WHEN `/api/check` arrives with no entry THEN a run starts.
GIVEN `client_budget_per_hour = 1` WHEN a prefetch request is refused and then a normal request arrives THEN the normal request starts a run.
GIVEN no run in flight WHEN `admit` is called with `prefetch: true` THEN it answers `NotStarted` and no throttle record is written; GIVEN a run in flight THEN it answers `Admitted` with that run's stream; GIVEN a throttled domain THEN it answers `NotStarted`, never `Refused`.
GIVEN header values `prefetch`, `prerender`, `prefetch;prerender`, `PreFetch, x` WHEN `is_prefetch` reads them THEN it returns true; GIVEN `none`, `prefetching`, an empty value or no header THEN false.
GIVEN the amended SDD WHEN read THEN §4.4 names the `[admission]` keys, the order, the refusal answers, the V1 holder and the prefetch token rule, §9.2's `netray_runs_total` lists the triggers visit, recheck, api, badge and og, and the Phase 2 table marks V2.6 as built by `v2-admission`.

## Decision log

- The history decides every hit; moka at most holds the V1 rendering: S17/§4.4 "no second
  cache". The V1 wire is removed at the cutover, so anything kept for it is transitional (spec
  drafter, 2026-10-17).
- One cache window (`cache.ttl_seconds`, 300 s) for all routes, over keeping
  `badges.ttl_seconds` and `OG_TTL_SECONDS` as windows: §4.4 names one window, and moka's TTL
  already capped them at 300 s, so the observed behaviour is the same (spec drafter, 2026-10-17).
- Runs execute in spawned tasks and record after publishing, over recording inline: it closes the
  `v2-history` deferred finding (`report.md:179`) and makes joining natural (spec drafter,
  2026-10-17).
- The throttle is a 60 s constant on tokio's clock, over a config key: §4.4 fixes "per minute" and
  S33 names only four config values. Tests advance tokio's paused clock instead of waiting
  (spec drafter, 2026-10-17).
- The per-IP and global limits stay in front of the cache lookup, over moving them to fresh runs
  only: today's behaviour, and existing 429 tests keep their meaning (implementer under the
  operator's autonomy mandate, 2026-10-17).
- V1 `/api/check` refusals answer 429 with `Retry-After`, over serving the head entry: §4.4 "the
  API answers 429", and a stale head has no V1 rendering under the moka holder (spec drafter,
  2026-10-17).
- Badge and OG refusals render the head entry, over `?`: §4.4 "a refused run gets the latest
  entry", and §9.1 says badge and OG show the last stored entry after an incomplete run (spec
  drafter, 2026-10-17).
- The slot is taken before the budget token, and the throttle minute starts only when a run
  starts: a refusal costs the client nothing (spec drafter, 2026-10-17).
- The admission keys live in lens config `[admission]`, over `netray.toml`: `netray.toml` arrives
  with the cutover (S10), and config stays per service until then (repo `CLAUDE.md`).
- The prefetch rule runs after the hit and join steps, over refusing every prefetch: S8 forbids
  starting a run, not reading a stored or running one (spec drafter, 2026-10-17).
- A V1 cache hit body comes from moka holding only the head's V1 rendering, keyed by entry id,
  over a `v1_bodies` side table in `history.db` or projecting the `Run` into the V1 shape: hits
  stay byte-identical with no migration the cutover must drop; a restart or eviction costs one
  re-run (operator, 2026-10-17).
- Badge and OG runs do not spend the per-client budget, over spending it as §4.4 reads literally:
  image proxies share one address across many sites, and badge and OG are already held to the
  recompute limiter and the slots (operator, 2026-10-17).
- Runs with `dkim_selectors` are bound by and start the per-domain throttle, over binding them
  only by the per-IP limit, budget and slots: "whatever started it" protects the target domain
  from runs varied by selector (operator, 2026-10-17).
- The admission path is two public functions, `check_limits` and `admit`, over one `admit` that
  also applies the per-IP and global limits: `/api/check` must spend those limits before its
  cache hit, which sits between the two, and `v2-api` carries its own per-IP limiter (spec
  drafter, 2026-10-17, aligning `v2-result-page` requirement 19).
- `admit` never reads the cache window, over a `visit` trigger that answers hits: the window is
  each caller's step (the result page renders the stored entry itself), so `visit` and `recheck`
  differ only for the caller and a re-check bypasses the window by construction (spec drafter,
  2026-10-17).
- A budget of 0 is valid and refuses every budgeted run with `retry_after_s` 3600, over refusing
  0 at startup: `v2-result-page` seeds its screenshots' refusal state with it, and governor
  takes no zero quota, so 0 builds no limiter (spec drafter, 2026-10-17).
- The run stream publishes `Done` before `Recorded`, over one final event carrying both: V1
  responses keep not waiting on the store, and `v2-api` and `v2-result-page` read on until
  `Recorded` for the entry id (spec drafter, 2026-10-17).
- Refusal reasons keep `ip_limit` and `global_limit` apart while the metric folds them into
  `limit`: `v2-result-page` words them differently, and `v2-cutover` names the four metric
  reasons throttle, budget, limit and slots (spec drafter, 2026-10-17).
- `DomainRuns` is defined once by `v2-api` in `netray_web::api` with `validate` and `start(domain,
  client, trigger, prefetch)`, and lens implements `start` over `v2-admission`'s `check_limits`
  then `admit`, over `v2-result-page` extending the trait: one owner for the one seam from
  netray-web to lens, built in lane A before lane C uses it (implementer under the operator's autonomy mandate, 2026-10-17).
- A `RunStream` `Section` carries the engine's `netray_engine::SectionEvent`, whose outcome holds
  the section's checks with findings and evidence references and its blocks, over an
  admission-specific section event: a section fragment renders from that event alone, and the
  engine already sends it (implementer under the operator's autonomy mandate, 2026-10-17).
- No V1, badge or OG response waits on the history store, and only `v2-api`'s and
  `v2-result-page`'s run answers read on to `Recorded`, over v1 answering at `Done` without
  `recorded` and `entry_id`: `v2-api` requirement 14's run answer
  (`{recorded, entry_id, run, diff}`) needs the record outcome, and the V1 routes
  stop at `Done` (implementer under the operator's autonomy mandate, 2026-10-17).
- `Trigger` gains `api`, spending the budget like `visit` and `recheck`, over v1's run POST passing
  `recheck`: SDD §9.2's `netray_runs_total{trigger}` names `api`, which `v2-cutover` counts (implementer under the operator's autonomy mandate, 2026-10-17).
- M1: `netray_runs_total{trigger}` gains `badge` and `og` beside visit, recheck and api, and this
  feature amends SDD §9.2 (requirement 19), over leaving badge and OG runs uncounted or folded into
  `visit`: this feature emits the triggers, so it owns the amendment, and `v2-cutover`'s metric row
  lists them (implementer under the operator's autonomy mandate, 2026-10-17).
- `is_prefetch` lives in `netray-common` (`netray_common::prefetch::is_prefetch`), over lens's
  `admission.rs`: netray-web's run and inspector routes decide the prefetch flag with the same
  helper, and netray-web cannot depend on lens (implementer under the operator's autonomy mandate, 2026-10-17).
- One build order in every spec (lane A `v2-history` → `v2-admission` → `v2-entries` → `v2-api`;
  lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C, after the operator's UI review at
  midday 2026-10-18, `v2-inspectors`, its Phase 2 before `v2-result-page` Phase 1, then
  `v2-result-page`; then `v2-cutover`) over each spec's own reading of the program: the order and
  the midday review are the operator's program (operator decision 2026-10-17); every **Depends on**
  and Context names the same order (implementer under the operator's autonomy mandate,
  2026-10-17).
- Released only within `0.30.0`, over cutting `0.27.0`–`0.29.0` per phase: only `0.30.0` is
  released, and S37 is amended by `v2-cutover`; this spec amends nothing in S37 (operator decision
  2026-10-17).
- `[admission]` key names `client_budget_per_hour` and `domain_run_slots` over S33's prose names
  or a `[throttle]` table: they state unit and scope in the key, and `client_budget_per_hour` is
  the name `v2-result-page` already uses (implementer under the operator's autonomy mandate,
  2026-10-17).
- Budget burst equal to the hourly quota over burst 1: a client may spend its 30 runs as a burst
  at once, as S33's "30 per hour" reads, while burst 1 would space runs two minutes apart
  (implementer under the operator's autonomy mandate, 2026-10-17).
- A `slots` refusal carries `retry_after_s` = 1 over an estimated run duration: a slot frees when
  any run ends, which lens cannot predict, and 1 s lets the client retry soon without a busy loop
  (implementer under the operator's autonomy mandate, 2026-10-17).
- Metric name `lens_admission_rejections_total` over SDD §9.2's `netray_admission_rejections_total`
  now: lens's series keep the `lens_` prefix until the cutover renames them, and the reasons are
  §9.2's (implementer under the operator's autonomy mandate, 2026-10-17).
- A refused prefetch on `/api/check` answers 503 `PREFETCH_REFUSED` with `Cache-Control:
  no-store` over 429 or 204: it is no rate refusal, an empty 204 would be cached as a result, and
  `no-store` keeps the browser from serving the refusal on navigation (implementer under the
  operator's autonomy mandate, 2026-10-17).
- S33 amended in Phase 1, with the `[admission]` keys, over in Phase 4 as the phases first read: the
  repo rule amends the SDD in the commit whose code changes what it says, and Phase 1 makes the
  four numbers lens config values (implementer under the operator's autonomy mandate, 2026-10-17).

- A run task carries the tracing span of the request that started it, over a span of its own:
  logging-rules C1 asks every log line of a request to carry its correlation fields, and a joiner's
  request adds no work of its own to log (implementer under the operator's autonomy mandate,
  2026-10-17).
- lens drains its run tasks on shutdown, bounded by the hard deadline plus 10 s, over letting the
  runtime drop them: the record now follows the response, and R3 promises a run whose requester
  has gone still records (implementer under the operator's autonomy mandate, 2026-10-17).

## Open decisions

None.

## Out of scope

- `/r/<id>` entries, the 410 page, privacy text: `v2-entries` (SDD §4.6–4.7, V2.7–V2.8).
- API v1's `POST /domains/{domain}/runs`, its 429 and the mapping of the five refusal reasons and
  `NotStarted` onto its error codes, the window bypass of its POST, its own per-IP limiter, and
  the `DomainRuns` trait lens implements over `check_limits` and `admit`: `v2-api` (SDD §7.1, its
  requirements 7 and 14).
- S26's refusal line and its wording per reason, "re-check now", the run POST calling
  `DomainRuns::start`, and rendering the run stream as SSE fragments: `v2-result-page`
  (SDD §5.2–5.3); the `stream` island: `v2-pages` (X3).
- Switching the OG card to `spectrum::layout`: `v2-result-page` (X4); OG keeps the V1 layout here.
- Where `cache.ttl_seconds` (the window) and `[admission]` land in `netray.toml`, and the legacy
  rows for `[cache]` and `[rate_limit]`: `v2-cutover`.
- Removing moka, `CachedResult` and the V1 wire: the cutover (SDD §7.4, Phase 5).
- Deploying `[admission]` and `[history] db_path` on the persistent volume: netray-ops.
- The S37 amendment for the single `0.30.0` release: the operator's release decision of
  2026-10-17, not this feature.
- The `v2-history` deferred finding on re-canonicalising with a changed volatile list
  (`specs/features/v2-history/report.md:226`): history semantics, not admission.
