//! The run and admission metrics of SDD v2 section 9.2 and the operational series, with their
//! histogram buckets. Copied from lens's `metrics.rs` (spec v2-cutover, Phase 6).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::Duration;

/// Bucket bounds: run durations and time to result span a fast cached resolve to the hard
/// deadline; time to first section is the fastest module; the run interval spans a minute (the
/// throttle) to a month (retention).
pub const HISTOGRAM_BUCKETS: &[(&str, &[f64])] = &[
    (
        "netray_run_duration_seconds",
        &[0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 30.0],
    ),
    (
        "netray_client_hourly_runs",
        &[1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 50.0, 100.0],
    ),
    (
        "netray_time_to_first_section_seconds",
        &[0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0],
    ),
    (
        "netray_time_to_result_seconds",
        &[0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 30.0],
    ),
    (
        "netray_run_interval_seconds",
        &[
            60.0,
            300.0,
            900.0,
            3600.0,
            21_600.0,
            86_400.0,
            604_800.0,
            2_592_000.0,
        ],
    ),
];

const TRIGGERS: [&str; 5] = ["visit", "recheck", "api", "badge", "og"];
const SECTIONS: [&str; 5] = ["dns", "tls", "http", "email", "ip"];
const SECTION_OUTCOMES: [&str; 4] = ["measured", "incomplete", "not_applicable", "not_tested"];

/// Describes every series of SDD v2 section 9.2 and the operational ones, and creates the
/// counters, histograms and gauges at zero so they render before the first request.
pub fn init_zero_series() {
    metrics::describe_counter!(
        "netray_runs_total",
        "Runs started, by outcome and what asked for them"
    );
    for outcome in ["complete", "incomplete"] {
        for trigger in TRIGGERS {
            metrics::counter!("netray_runs_total", "outcome" => outcome, "trigger" => trigger)
                .increment(0);
        }
    }
    metrics::describe_counter!(
        "netray_runs_with_predecessor_total",
        "Recorded runs of a domain that had an earlier run"
    );
    metrics::counter!("netray_runs_with_predecessor_total").increment(0);
    metrics::describe_counter!(
        "netray_section_outcomes_total",
        "Section outcomes of finished runs"
    );
    for section in SECTIONS {
        for outcome in SECTION_OUTCOMES {
            metrics::counter!("netray_section_outcomes_total", "section" => section, "outcome" => outcome)
                .increment(0);
        }
    }
    metrics::describe_histogram!(
        "netray_time_to_first_section_seconds",
        "Seconds from the start of a run to its first finished section"
    );
    let _ = metrics::histogram!("netray_time_to_first_section_seconds");
    metrics::describe_histogram!(
        "netray_time_to_result_seconds",
        "Seconds from the start of a run to its result"
    );
    let _ = metrics::histogram!("netray_time_to_result_seconds");
    metrics::describe_histogram!(
        "netray_run_interval_seconds",
        "Seconds since the domain's previous run, observed when a run is recorded"
    );
    let _ = metrics::histogram!("netray_run_interval_seconds");
    metrics::describe_counter!(
        "netray_fixes_observed_total",
        "Fixed check transitions in recorded diffs, by section"
    );
    for section in SECTIONS {
        metrics::counter!("netray_fixes_observed_total", "section" => section).increment(0);
    }
    metrics::describe_counter!(
        "netray_badge_requests_total",
        "Badge requests, by cache result"
    );
    for cache in ["hit", "miss", "throttled", "failed"] {
        metrics::counter!("netray_badge_requests_total", "cache" => cache).increment(0);
    }
    metrics::describe_histogram!(
        "netray_badge_render_duration_seconds",
        "Seconds to render a badge SVG"
    );
    metrics::describe_counter!("netray_entry_views_total", "Views of a stored run's page");
    metrics::counter!("netray_entry_views_total").increment(0);
    metrics::describe_counter!(
        "netray_api_requests_total",
        "API requests, by route template"
    );
    for endpoint in ["/api/v1/domains/{domain}/runs", "/api/v1/domains/{domain}"] {
        metrics::counter!("netray_api_requests_total", "endpoint" => endpoint).increment(0);
    }
    metrics::describe_counter!(
        "netray_admission_rejections_total",
        "Requests refused a run, by reason"
    );
    for reason in ["throttle", "budget", "limit", "slots"] {
        metrics::counter!("netray_admission_rejections_total", "reason" => reason).increment(0);
    }
    metrics::describe_gauge!(
        "netray_history_entries",
        "Entries held in the history store"
    );
    metrics::describe_gauge!(
        "netray_history_domains",
        "Distinct domains held in the history store"
    );
    metrics::gauge!("netray_history_entries").set(0.0);
    metrics::gauge!("netray_history_domains").set(0.0);
    metrics::describe_counter!(
        "netray_rate_limit_hits_total",
        "Requests refused by a rate limiter, by scope"
    );
    for scope in [
        "per_ip",
        "global",
        "badge",
        "og",
        "api_v1",
        "api_v1_ip",
        "api_v1_ip_target",
        "api_v1_ip_batch",
    ] {
        metrics::counter!("netray_rate_limit_hits_total", "scope" => scope).increment(0);
    }
    metrics::describe_gauge!("netray_runs_in_flight", "Runs in flight");
    metrics::gauge!("netray_runs_in_flight").set(0.0);
    metrics::describe_counter!(
        "netray_history_record_errors_total",
        "Runs the history store failed to record"
    );
    metrics::counter!("netray_history_record_errors_total").increment(0);
    metrics::describe_histogram!(
        "netray_run_duration_seconds",
        "Seconds a visit, recheck or API run took"
    );
    let _ = metrics::histogram!("netray_run_duration_seconds");
    metrics::describe_histogram!(
        "netray_client_hourly_runs",
        "Fresh check runs per client in one flushed hour, one observation per active client; a restart loses the partial hour"
    );
    let _ = metrics::histogram!("netray_client_hourly_runs");
}

/// Counts a finished run and its sections. A protocol the run has no section for counts
/// nothing. A measured section counts as the scorer sees it: `incomplete` when a weighted
/// check is unmeasured or nothing weighted was scored, `not_tested` when no check was tested
/// (the section result maps `NotTested` onto `Skip`, so the run records it as
/// `NotApplicable`).
pub fn count_run(
    trigger: &'static str,
    run: &netray_model::run::Run,
    profile: &netray_engine::profile::Profile,
) {
    use netray_model::Status;
    use netray_model::run::SectionKind;
    let outcome = if run.complete {
        "complete"
    } else {
        "incomplete"
    };
    metrics::counter!("netray_runs_total", "outcome" => outcome, "trigger" => trigger).increment(1);
    for section in SECTIONS {
        let Some(s) = run.sections.iter().find(|s| s.protocol.as_str() == section) else {
            continue;
        };
        let outcome = match s.outcome {
            SectionKind::NotApplicable => "not_applicable",
            SectionKind::Incomplete | SectionKind::TimedOut => "incomplete",
            SectionKind::Measured => {
                let checks: Vec<_> = run
                    .checks
                    .iter()
                    .filter(|c| c.id.protocol() == s.protocol)
                    .collect();
                let weights = profile.sections.get(&s.protocol).map(|w| &w.checks);
                let mut scored = false;
                let mut unmeasured = false;
                for c in &checks {
                    if !weights.is_some_and(|w| w.contains_key(&c.id)) {
                        continue;
                    }
                    match c.status {
                        Status::Unmeasured => unmeasured = true,
                        Status::Pass | Status::Warn | Status::Fail => scored = true,
                        Status::NotApplicable | Status::NotTested => {}
                    }
                }
                let untested = |c: &&netray_model::run::RunCheck| {
                    matches!(c.status, Status::NotTested | Status::NotApplicable)
                };
                if !checks.is_empty() && checks.iter().all(untested) {
                    "not_tested"
                } else if weights.is_some() && (unmeasured || !scored) {
                    "incomplete"
                } else {
                    "measured"
                }
            }
        };
        metrics::counter!("netray_section_outcomes_total", "section" => section, "outcome" => outcome)
            .increment(1);
    }
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
            metrics::histogram!("netray_client_hourly_runs").record(f64::from(count));
        }
    }
}

pub fn observe_run(duration: Duration) {
    metrics::histogram!("netray_run_duration_seconds").record(duration.as_secs_f64());
}

/// Holds `netray_runs_in_flight` up for as long as it lives, including when the
/// owning future is dropped mid-run.
pub struct RunGuard(());

impl RunGuard {
    pub fn new() -> Self {
        metrics::gauge!("netray_runs_in_flight").increment(1.0);
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
        metrics::gauge!("netray_runs_in_flight").decrement(1.0);
    }
}

/// Installs the global Prometheus recorder with [`HISTOGRAM_BUCKETS`] and creates every series
/// at zero. Call it before building the apex, so the startup gauges land on it.
pub fn install_metrics() -> netray_common::server::PrometheusHandle {
    netray_common::server::install_metrics(HISTOGRAM_BUCKETS, init_zero_series)
        .expect("failed to install the metrics recorder")
}
