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
