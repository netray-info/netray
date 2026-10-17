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
