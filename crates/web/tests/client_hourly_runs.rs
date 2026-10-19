//! Per-client hourly run counts (specs/features/lens-admission-metrics/spec.md, Phase 2, R3;
//! criteria C2 to C6).
//!
//! The metrics are read from a per-test Prometheus recorder, non-destructively through its
//! handle; a current-thread runtime keeps the handler on the recording thread.
//!
//! Copied from crates/lens/tests/client_hourly_runs.rs (spec v2-cutover, Phase 6); dropped as V1:
//! `POST /api/check` and its V1 cache hit. C4 runs on API v1's `POST /api/v1/domains/{domain}/runs`;
//! its hit is the result page's visit of a fresh head (`POST /{domain}/run`, a stored answer), its
//! refusal a throttled repeat.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

mod common;

use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::registry;
use netray_common::server::metrics_recorder;
use netray_store::HistoryStore;
use netray_web::admission::ApexState;
use netray_web::metrics::{ClientRunCounter, HISTOGRAM_BUCKETS, init_zero_series};
use tower::ServiceExt;

const NAME: &str = "netray_client_hourly_runs";

fn ip(last: u8) -> IpAddr {
    IpAddr::from([192, 0, 2, last])
}

/// A counter flushed after A ran three times and B once.
fn flushed_a3_b1() {
    let counter = ClientRunCounter::new();
    for _ in 0..3 {
        counter.record(ip(1));
    }
    counter.record(ip(2));
    counter.flush();
}

#[test]
fn client_runs_c2_flush_observes_one_value_per_client() {
    let recorder = metrics_recorder(HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);

    flushed_a3_b1();
    let out = handle.render();

    for line in [
        format!("{NAME}_count 2"),
        format!("{NAME}_sum 4"),
        format!(r#"{NAME}_bucket{{le="1"}} 1"#),
        format!(r#"{NAME}_bucket{{le="3"}} 2"#),
    ] {
        assert!(
            out.lines().any(|l| l == line),
            "missing `{line}` in:\n{out}"
        );
    }
    for le in ["2", "5", "10", "20", "50", "100", "+Inf"] {
        let prefix = format!(r#"{NAME}_bucket{{le="{le}"}} "#);
        assert!(
            out.lines().any(|l| l.starts_with(&prefix)),
            "missing bucket le={le} in:\n{out}"
        );
    }
}

#[test]
fn client_runs_c3_second_flush_without_runs_adds_no_observation() {
    let recorder = metrics_recorder(HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);

    let counter = ClientRunCounter::new();
    counter.record(ip(1));
    counter.record(ip(2));
    counter.flush();
    counter.flush();
    let out = handle.render();

    assert!(
        out.lines().any(|l| l == format!("{NAME}_count 2")),
        "count must stay 2 in:\n{out}"
    );
    assert!(out.lines().any(|l| l == format!("{NAME}_sum 2")), "{out}");
}

#[test]
fn client_runs_c5_render_carries_no_client_address() {
    let recorder = metrics_recorder(HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);

    flushed_a3_b1();
    let out = handle.render();

    assert!(
        out.contains(&format!("{NAME}_count 2")),
        "the flush observed: {out}"
    );
    assert!(!out.contains("192.0.2.1"), "address leaked:\n{out}");
    assert!(!out.contains("192.0.2.2"), "address leaked:\n{out}");
}

#[test]
fn client_runs_c6_help_text_names_restart_and_partial_hour() {
    let recorder = metrics_recorder(HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);

    init_zero_series();
    let out = handle.render();

    let help = out
        .lines()
        .find(|l| l.starts_with(&format!("# HELP {NAME} ")))
        .unwrap_or_else(|| panic!("no HELP line for {NAME} in:\n{out}"));
    assert!(help.contains("restart"), "{help}");
    assert!(help.contains("partial hour"), "{help}");
}

// Router-driven criterion, modelled on admission_metrics.rs.

fn app(per_ip: u32, store: Option<Arc<HistoryStore>>) -> (Router, ApexState) {
    let mut config = common::production_config();
    assert!(config.cache_enabled, "production config enables the cache");
    config.limits.rate_limit_per_ip_per_minute = per_ip;
    config.limits.rate_limit_per_ip_burst = per_ip;

    let registry = registry(Some("spectra-inspect.json"), "beacon.sse");
    let plan = common::production_plan(&registry);
    let mut state = common::state_with(config, registry, plan);
    state.history_store = store;
    let router = netray_web::host::apex::routes(state.clone()).layer(Extension(ConnectInfo(
        SocketAddr::from(([127, 0, 0, 1], 40000)),
    )));
    (router, state)
}

async fn send(app: &Router, req: Request<Body>) -> StatusCode {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    to_bytes(resp.into_body(), 16 * 1024 * 1024).await.unwrap();
    status
}

/// API v1's fresh run of `domain`.
async fn post_run(app: &Router, domain: &str) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/domains/{domain}/runs"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// The result page's visit of `domain`: a fresh head answers stored, without a run.
async fn visit(app: &Router, domain: &str) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/{domain}/run"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ACCEPT, "text/event-stream")
        .body(Body::from("trigger=visit"))
        .unwrap();
    send(app, req).await
}

#[tokio::test(flavor = "current_thread")]
async fn client_runs_c4_only_the_fresh_run_counts() {
    let recorder = metrics_recorder(HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    // Burst of 2: the fresh run passes, the stored answer spends nothing, the throttled repeat
    // is refused.
    let (store, _dir) = common::history_store().await;
    let (app, state) = app(2, Some(store.clone()));

    assert_eq!(post_run(&app, "example.com").await, StatusCode::OK);
    let mut held = false;
    for _ in 0..250 {
        if store.head("example.com").await.unwrap().is_some() {
            held = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(held, "the run was not recorded within 5 s");
    assert_eq!(visit(&app, "example.com").await, StatusCode::OK); // stored answer
    assert_eq!(
        post_run(&app, "example.com").await,
        StatusCode::TOO_MANY_REQUESTS
    );
    state.client_runs.flush();
    let out = handle.render();

    assert!(
        out.lines().any(|l| l == format!("{NAME}_count 1")),
        "one client observed in:\n{out}"
    );
    assert!(
        out.lines().any(|l| l == format!("{NAME}_sum 1")),
        "one fresh run in:\n{out}"
    );
}
