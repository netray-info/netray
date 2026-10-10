use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::Duration;

pub const HISTOGRAM_BUCKETS: &[(&str, &[f64])] = &[
    (
        "lens_run_duration_seconds",
        &[0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 30.0],
    ),
    (
        "lens_client_hourly_runs",
        &[1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 50.0, 100.0],
    ),
];

/// Creates the admission series at zero so they render before the first request.
pub fn init_zero_series() {
    for result in ["rate_limited", "cache_hit", "fresh"] {
        metrics::counter!("lens_check_requests_total", "result" => result).increment(0);
    }
    for cache in ["hit", "miss", "throttled", "failed"] {
        metrics::counter!("lens_badge_requests_total", "cache" => cache).increment(0);
    }
    for scope in ["per_ip", "global", "badge", "og"] {
        metrics::counter!("lens_rate_limit_hits_total", "scope" => scope).increment(0);
    }
    metrics::gauge!("lens_runs_in_flight").set(0.0);
    metrics::describe_histogram!(
        "lens_client_hourly_runs",
        "Fresh check runs per client in one flushed hour, one observation per active client; a restart loses the partial hour"
    );
    let _ = metrics::histogram!("lens_client_hourly_runs");
}

/// Counts fresh runs per client address for the current hour. The addresses
/// stay in memory only: `flush` records one count per client and drops the keys.
#[derive(Default)]
pub struct ClientRunCounter {
    runs: Mutex<HashMap<IpAddr, u32>>,
}

impl ClientRunCounter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, ip: IpAddr) {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        *runs.entry(ip).or_insert(0) += 1;
    }

    pub fn flush(&self) {
        let runs = std::mem::take(&mut *self.runs.lock().unwrap_or_else(|e| e.into_inner()));
        for count in runs.into_values() {
            metrics::histogram!("lens_client_hourly_runs").record(f64::from(count));
        }
    }
}

pub fn count_request(result: &'static str) {
    metrics::counter!("lens_check_requests_total", "result" => result).increment(1);
}

pub fn observe_run(duration: Duration) {
    metrics::histogram!("lens_run_duration_seconds").record(duration.as_secs_f64());
}

/// Holds `lens_runs_in_flight` up for as long as it lives, including when the
/// owning future is dropped mid-run.
pub struct RunGuard(());

impl RunGuard {
    pub fn new() -> Self {
        metrics::gauge!("lens_runs_in_flight").increment(1.0);
        Self(())
    }
}

impl Default for RunGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        metrics::gauge!("lens_runs_in_flight").decrement(1.0);
    }
}
