// Contract: `init_zero_series()` creates lens's admission series at zero on the current
// recorder, and `HISTOGRAM_BUCKETS` carries lens's bucket config.
// (specs/features/lens-admission-metrics/spec.md, R4: C12.)

use lens::metrics::{HISTOGRAM_BUCKETS, init_zero_series};
use netray_common::server::metrics_recorder;

#[test]
fn zero_series_render_at_zero_and_buckets_are_configured() {
    let recorder = metrics_recorder(HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);

    init_zero_series();
    let out = handle.render();

    let expected = [
        r#"lens_check_requests_total{result="rate_limited"} 0"#,
        r#"lens_check_requests_total{result="cache_hit"} 0"#,
        r#"lens_check_requests_total{result="fresh"} 0"#,
        r#"lens_badge_requests_total{cache="hit"} 0"#,
        r#"lens_badge_requests_total{cache="miss"} 0"#,
        r#"lens_badge_requests_total{cache="throttled"} 0"#,
        r#"lens_badge_requests_total{cache="failed"} 0"#,
        r#"lens_rate_limit_hits_total{scope="per_ip"} 0"#,
        r#"lens_rate_limit_hits_total{scope="global"} 0"#,
        r#"lens_rate_limit_hits_total{scope="badge"} 0"#,
        r#"lens_rate_limit_hits_total{scope="og"} 0"#,
        "lens_runs_in_flight 0",
    ];
    let missing: Vec<&str> = expected
        .iter()
        .copied()
        .filter(|line| !out.lines().any(|l| l == *line))
        .collect();
    assert!(
        missing.is_empty(),
        "missing zero series {missing:?} in:\n{out}"
    );

    let buckets = |name: &str| -> Vec<f64> {
        HISTOGRAM_BUCKETS
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("HISTOGRAM_BUCKETS lacks {name}"))
            .1
            .to_vec()
    };
    assert_eq!(
        buckets("lens_run_duration_seconds"),
        vec![0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 30.0]
    );
    assert_eq!(
        buckets("lens_client_hourly_runs"),
        vec![1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 50.0, 100.0]
    );
}
