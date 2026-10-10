//! `metrics_recorder` builds (does not install) a Prometheus recorder whose histograms
//! named in `buckets` render with those buckets (spec lens-admission-metrics R5, C4, C13).

use netray_common::server::metrics_recorder;

fn render_after_recording(buckets: &[(&str, &[f64])]) -> String {
    let recorder = metrics_recorder(buckets);
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, || {
        metrics::histogram!("x_seconds").record(2.0);
    });
    handle.render()
}

#[test]
fn configured_histogram_renders_with_the_given_buckets() {
    let out = render_after_recording(&[("x_seconds", &[1.0, 5.0])]);
    assert!(out.contains("x_seconds_bucket{le=\"1\"} 0"), "{out}");
    assert!(out.contains("x_seconds_bucket{le=\"5\"} 1"), "{out}");
    assert!(out.contains("x_seconds_bucket{le=\"+Inf\"} 1"), "{out}");
}

#[test]
fn empty_buckets_render_a_summary() {
    let out = render_after_recording(&[]);
    assert!(out.contains("quantile="), "{out}");
    assert!(!out.contains("x_seconds_bucket"), "{out}");
}
