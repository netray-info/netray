//! Per-client hourly run counts (specs/features/lens-admission-metrics/spec.md, Phase 2, R3;
//! criteria C2 to C6).
//!
//! The metrics are read from a per-test Prometheus recorder, non-destructively through its
//! handle; a current-thread runtime keeps the handler on the recording thread.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use common::registry;
use lens::config::Config;
use lens::metrics::{ClientRunCounter, HISTOGRAM_BUCKETS, init_zero_series};
use lens::routes::api_router;
use lens::state::AppState;
use netray_common::server::metrics_recorder;
use tower::ServiceExt;

const NAME: &str = "lens_client_hourly_runs";

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

fn golden(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()))
}

async fn stub(
    path: &'static str,
    is_post: bool,
    content_type: &'static str,
    golden_name: &'static str,
) -> String {
    let body = golden(golden_name);
    let handler = move || {
        let body = body.clone();
        async move { ([(header::CONTENT_TYPE, content_type)], body) }
    };
    let route = if is_post { post(handler) } else { get(handler) };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let app = Router::new().route(path, route);
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://{addr}")
}

async fn app(per_ip: u32) -> (Router, AppState) {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.backends.dns.url =
        Some(stub("/api/check", true, "text/event-stream", "prism.sse").await);
    assert!(config.cache.enabled, "production config enables the cache");
    config.rate_limit.per_ip_per_minute = per_ip;
    config.rate_limit.per_ip_burst = per_ip;

    let state =
        AppState::with_registry(config, registry(Some("spectra-inspect.json"), "beacon.sse"))
            .unwrap();
    let (api, _) = api_router().split_for_parts();
    let router = Router::new()
        .merge(api.with_state(state.clone()))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    (router, state)
}

async fn post_check(app: &Router, domain: &str) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"domain":"{domain}"}}"#)))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    status
}

#[tokio::test(flavor = "current_thread")]
async fn client_runs_c4_only_the_fresh_run_counts() {
    let recorder = metrics_recorder(HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    // Burst of 2: the fresh run and the cached repeat pass, the third request is limited.
    let (app, state) = app(2).await;

    assert_eq!(post_check(&app, "example.com").await, StatusCode::OK);
    assert_eq!(post_check(&app, "example.com").await, StatusCode::OK); // cache hit
    assert_eq!(
        post_check(&app, "example.com").await,
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
