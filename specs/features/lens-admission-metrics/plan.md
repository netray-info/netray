# Plan: lens admission metrics

## Phase 1 — Request, run and zero series

## Groups

G1: C4, C13 (common) · G2: C1–C3, C6–C12 (lens, depends on G1) · G3: C5, C14 (already_implemented, verification only). Written by the orchestrator: two groups along the crate boundary.

## Plan

### G1
- `crates/common/src/server.rs` (`prometheus_server`): `pub fn metrics_recorder(buckets: &[(&str, &[f64])]) -> PrometheusRecorder` (builder with `set_buckets_for_metric(Matcher::Full(name), buckets)` per entry, `build_recorder()`); `pub async fn serve_metrics_with(addr, shutdown, buckets, after_install: impl FnOnce() + Send)`: build, `metrics::set_global_recorder`, run `after_install`, serve the handle; `serve_metrics(addr, shutdown)` = `serve_metrics_with(addr, shutdown, &[], || {})`. Re-export beside `serve_metrics`.
- `crates/common/Cargo.toml`: `[[test]] name = "metrics_recorder" required-features = ["prometheus"]`.

### G2
- `crates/lens/src/metrics.rs` (new, `pub mod metrics` in `lib.rs`): `HISTOGRAM_BUCKETS` (`lens_run_duration_seconds` 0.5 1 2 5 10 15 20 30; `lens_client_hourly_runs` 1 2 3 5 10 20 50 100), `init_zero_series()`, `count_request(result)`, an in-flight guard (`RunGuard`: increments on new, decrements on drop) and `observe_run(duration)`.
- `crates/lens/src/routes.rs` `run_check_handler`: after validation, count `rate_limited` on a limiter rejection, `cache_hit` on a cache answer, `fresh` before a run; wrap `run_check_with_input` in the guard and observe its wall time.
- `crates/lens/src/lib.rs`: `serve_metrics_with(metrics_addr, shutdown, HISTOGRAM_BUCKETS, init_zero_series)`.

## Phase 2 — Hourly runs per client

## Groups

G1: C1–C6 (lens only; written by the orchestrator, one group)

## Plan

### G1
- `crates/lens/src/metrics.rs`: `ClientRunCounter` (a `Mutex<HashMap<IpAddr, u32>>`; `new`, `record`, `flush` observing `lens_client_hourly_runs` per client and clearing); `init_zero_series` describes `lens_client_hourly_runs` with the restart caveat.
- `crates/lens/src/state.rs`: `pub client_runs: Arc<ClientRunCounter>`, built in every constructor.
- `crates/lens/src/routes.rs` `run_check_handler`: `state.client_runs.record(client_ip)` beside the `fresh` count.
- `crates/lens/src/lib.rs`: spawn a task that ticks every hour (`tokio::time::interval`, first tick skipped) and calls `flush`, ending on shutdown.
