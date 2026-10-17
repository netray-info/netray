# Plan: v2-admission

## Phase 1 — Admission config

## Groups

G1: C1–C7 (one group: every criterion goes through `crates/lens/src/config.rs`; C6 also needs the production fixture; C4 rides on the existing `LENS_` env mapping, C7's defaults already hold).

## Plan

### G1
1. `crates/lens/src/config.rs`: `AdmissionConfig { client_budget_per_hour: u32 (30), domain_run_slots: u32 (8) }`, `deny_unknown_fields`, per-field serde defaults, `impl Default` after the `HistoryConfig` pattern; `#[serde(default)] pub admission` on `Config` after `rate_limit`.
2. `validate()`: `reject_zero("admission.domain_run_slots", …)`; no rule for `client_budget_per_hour` (0 is valid).
3. Config literals gain `admission: AdmissionConfig::default()`: `config.rs`, `state.rs`, `routes.rs` test modules and the integration-test literals (badge_routes, deadlines, history_badge, og_label_bounds, og_routes, reference_domains, sdd_lens_badges_p1_c11_cache_coalesce, sdd_lens_badges_p2_c5_meta_badges, snapshot_routes). Extension only; nothing weakened.
4. `crates/lens/tests/fixtures/lens.production.toml`: `[admission]` with both values after `[rate_limit]`; `crates/lens/lens.example.toml` mirrors it.

Group command: `cargo test -p lens --test admission_config && cargo build -p netray && bash tests/repo/test_check_config.sh`

## Phase 2 — One run per domain, recorded off the request path

## Groups

G1: C1–C20 (one group: every criterion needs `admission.rs`; moving `history::record` out of `check.rs` breaks recording until the routes use the task, so the change cannot be split).

## Plan

### G1
1. `check.rs`: `run_check_streaming(state, input, deadline, on_event)` forwards each engine `SectionEvent` live; `run_check_with_evidence` and its inline `history::record` go; public `run_check*` keep their signatures.
2. `history.rs`: `record(state, domain, dkim, &Run) -> Option<Recorded>`; logs and counts failures as today.
3. `admission.rs` (new): `Trigger`, `Published`, `RunEvent`, `Refusal`/`Reason` (unused until Phase 4), `Admit`, `Runs` (in-flight map of `watch::Receiver<RunState>` keyed by `cache_key`), `RunStream` (replays finished sections, then `Done`, `Recorded`, `None`), `admit` (join or register + spawn), `start_unregistered` (dkim path), `run_task` (deregistration guard, `RunGuard`+`observe_run` for visit/recheck/api, check or `badge_check_fn` for badge/og, snapshot when complete, `build_run` once, publish `Done`, record, publish `Recorded`, deregister), `until_done`.
4. `state.rs`: `runs: Arc<admission::Runs>`; `lib.rs`: `pub mod admission`; `metrics.rs`: `joined` zero series.
5. `routes.rs`: `create_snapshot` `pub(crate)`; renderers take `&CheckOutput`; `cache_published` replaces the inline moka store; `run_check_handler` keeps validate, limits, moka hit, then `admit(Visit)` or `start_unregistered` for dkim, counts `joined`/`fresh`, reads to `Done`; badge keeps the moka hit and recompute limiter, then `admit(Badge)`; `invoke_badge_check` goes.
6. `og/handler.rs`: as badge with `Trigger::Og`; `invoke_check` goes. `cache.rs`: `get_or_compute` goes.

Existing tests that race now that the record is off the request path (`history_record.rs` c1_c8, c3_c9, c5, c12) are amended by the orchestrator with `ADLC-Test-Change`.

Group command: `cargo test -p lens --test admission_join --test admission_record --test admission_stream --test admission_metrics`

## Phase 3 — The history is the cache

## Groups

G1: C2, C24 · G2: C23 · G3: C1, C3–C22 (G3 needs G1's `head` and G2's `score_run`; the `/api/check` and badge/OG criteria share `cache.rs` and `routes.rs`).

## Plan

### G1
1. `crates/store/src/lib.rs`: `Head { id, last_seen_at, grade, profile_version }`, `HistoryStore::head`; `head_in` widened (grade, profile_version; still no payload column), `sweep` reads it.

### G2
1. `crates/engine/src/history.rs`: `score_run(run, profile) -> Score` takes over the event rebuild; `rescore` = `score_run(..).grade`.

### G3
1. `lens/src/history.rs`: `now_unix` `pub(crate)`.
2. `lens/src/cache.rs`: `is_fresh`, `store_result`, `CachedResult::cached_at` go; `head(state, domain) -> Option<HeadLookup{head, fresh}>` (no store or error → none; fresh needs `cache.enabled` and `now - last_seen_at < ttl`); `hold(state, recorded, published)` inserts the V1 rendering under the entry id for First/Identical/Changed.
3. `lens/src/admission.rs`: `run_task` calls `cache::hold` after `record`, before publishing `Recorded`.
4. `lens/src/routes.rs`: `cache_published` goes; `/api/check` hit = fresh head + `cache.get(head.id)`; badge: fresh head → LONG; limiter refusal → head SHORT or `?`; run complete → LONG, incomplete → head SHORT or unscored SHORT; `badge_response` helper.
5. `lens/src/og/handler.rs`: `head_card` (entry run scored with `score_run` under the configured profile when versions match, else `Profiles::get(v)`, else the configured profile), same branches as badge; `og_response` helper. `og/layout.rs`: `OG_TTL_SECONDS` goes.
6. `lens/src/state.rs`: `cache` doc comment (V1 holder). `specs/sdd/v2.md` §4.4 first bullet: the V1 holder.

Existing tests whose meaning changes (sdd_lens_badges_p1_c11 :146, admission_metrics c7/c11, incomplete_results :197/:219/:324/:344, history_badge :142, state.rs `cache_built_when_enabled`, og_routes :189/:242, badge_routes :324, client_hourly_runs c4) are amended by the orchestrator with `ADLC-Test-Change`.

Group commands: G1 `cargo test -p netray-store --test head`; G2 `cargo test -p netray-engine --test score_run --test history`; G3 `cargo test -p lens --test history_cache --test history_cache_badge_og`.

## Phase 4 — Throttle, budget, slots

## Groups

G1: C24 · G2: C1–C23, C25–C32 (G2 needs G1's zero series; the already-implemented C15–C18, C27, C29 sit in G2 because the reordered `admit` could break them).

## Plan

### G1
1. `metrics.rs`: `lens_admission_rejections_total{reason}` zero series for throttle, budget, limit, slots.

### G2
1. `security/rate_limit.rs`: `check_rate_limit` returns the `RateLimitRejection` (scope, retry).
2. `state.rs`: `throttle: Arc<admission::Throttle>`, `slots: Arc<Semaphore>` (`domain_run_slots`), `client_budget: Option<Arc<ClientBudgetLimiter>>` (quota per hour, burst = quota; none for 0).
3. `admission.rs`: `THROTTLE`; `Throttle` records (tokio `Instant` start, completion outcome), pruned under the lock; `refused(..)` counts the reason once; `check_limits`; `gate` (throttle, slot `try_acquire_owned`, budget, then the record — a refusal leaves nothing behind) returning a `Ticket`; `admit`: join, (Phase 5 prefetch), the recompute limiter for badge/og, `gate`, start; `admit_dkim` replaces `start_unregistered`; `run_task` holds the ticket, marks the record's outcome before `Done`, releases the permit at its end.
4. `routes.rs`: `check_limits` in place of `check_rate_limit`; `refusal_response` (429 + `rate_limited`); dkim through `admit_dkim`; badge limiter block goes, `Refused` renders the head short or `?`, counted `throttled`.
5. `og/handler.rs`: limiter block goes; `Refused` renders the head card short or `?`, counted `throttled`.

Existing tests that now meet the throttle or the moved limiter are amended by the orchestrator (`ADLC-Test-Change`), with a `pass_throttle()` helper in `tests/common/mod.rs`.

Group commands: G1 `cargo test -p lens --test admission_slots c24`; G2 `cargo test -p lens --test admission_throttle --test admission_budget --test admission_slots`.
