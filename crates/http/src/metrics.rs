/// Register metric descriptions at startup.
///
/// Uses the `metrics` crate facade (v0.24); `metrics-exporter-prometheus` is
/// installed by `netray_common::server::serve_metrics`.
pub fn register_metrics() {
    metrics::describe_histogram!(
        "spectra_inspect_duration_ms",
        metrics::Unit::Milliseconds,
        "Inspection end-to-end duration in milliseconds"
    );
    metrics::describe_counter!(
        "spectra_probe_failures_total",
        "Number of probe failures by probe type"
    );
    metrics::describe_counter!(
        "spectra_inspect_requests_total",
        "Total number of inspection requests by outcome"
    );
}
