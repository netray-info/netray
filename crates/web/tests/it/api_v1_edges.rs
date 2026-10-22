//! `/api/v1` mount edges in `lens::app` (spec v2-api): trailing slash, body limit errors,
//! the scoring profile, the metrics namespace.
//! Copied from crates/lens/tests/api_v1_edges.rs (spec v2-cutover, Phase 6); the plain 413 is
//! sent to a non-v1 POST path instead of the dropped V1 `POST /api/check`.
use crate::common;

use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use netray_common::server::metrics_recorder;
use netray_engine::Registry;
use netray_engine::profile::Profile;
use netray_web::admission::{ApexState, load_scoring_profile};
use netray_web::api::AdmissionLimits;
use netray_web::config::ApexConfig;
use tower::ServiceExt;

/// lens's `base_config`: the cache off, 60 per minute and a burst of 10 per IP, 1000 and 100
/// globally.
fn base_config() -> ApexConfig {
    let defaults = ApexConfig::default();
    ApexConfig {
        cache_enabled: false,
        cache_window_seconds: 300,
        limits: AdmissionLimits {
            rate_limit_per_ip_per_minute: 60,
            rate_limit_per_ip_burst: 10,
            rate_limit_global_per_minute: 1000,
            rate_limit_global_burst: 100,
            ..defaults.limits
        },
        ..defaults
    }
}

fn app_with(config: ApexConfig, profile: Profile) -> Router {
    let registry = Registry::new();
    let plan = common::default_plan(&registry);
    netray_web::host::apex::app(ApexState::new(
        config,
        registry,
        plan,
        common::RESOLVE_BUDGET,
        profile,
    ))
}

fn app(config: ApexConfig) -> Router {
    app_with(config, common::profile())
}

async fn send(app: &Router, mut req: Request<Body>) -> (StatusCode, Option<String>, String) {
    let peer: SocketAddr = "203.0.113.9:4000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, ct, String::from_utf8_lossy(&bytes).into_owned())
}

fn is_json(ct: &Option<String>) -> bool {
    ct.as_deref()
        .is_some_and(|c| c.starts_with("application/json"))
}

#[tokio::test]
async fn trailing_slash_under_the_mount_is_a_json_404_not_a_page() {
    let (s, ct, body) = send(
        &app(base_config()),
        Request::get("/api/v1/").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(is_json(&ct), "content-type {ct:?}, body {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn oversized_body_is_a_json_413_on_v1_and_a_plain_413_elsewhere() {
    let app = app(base_config());
    let req = Request::get("/api/v1/catalogue")
        .header("content-length", "9000")
        .body(Body::from(vec![b'a'; 9000]))
        .unwrap();
    let (s, ct, body) = send(&app, req).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(is_json(&ct), "content-type {ct:?}, body {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error"]["code"], "PAYLOAD_TOO_LARGE");

    let req = Request::post("/example.com/run")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("content-length", "9000")
        .body(Body::from(vec![b'a'; 9000]))
        .unwrap();
    let (s, ct, body) = send(&app, req).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!is_json(&ct), "content-type {ct:?}, body {body}");
}

#[tokio::test]
async fn scoring_profile_path_sets_the_profile_v1_reports() {
    let text = include_str!("../../../engine/profiles/v3.toml");
    assert!(text.contains("version = 3"));
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), text.replacen("version = 3", "version = 4", 1)).unwrap();
    let profile = load_scoring_profile(Some(&file.path().to_string_lossy())).unwrap();
    let app = app_with(base_config(), profile);

    let (s, _, body) = send(
        &app,
        Request::get("/api/v1/meta").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["current_profile"], 4);

    let (s, _, body) = send(
        &app,
        Request::get("/api/v1/profiles/4")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
}

#[test]
fn zero_series_include_the_api_v1_rate_limit_scope() {
    let recorder = metrics_recorder(netray_web::metrics::HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    netray_web::metrics::init_zero_series();
    let out = handle.render();
    assert!(
        out.lines()
            .any(|l| l == r#"netray_rate_limit_hits_total{scope="api_v1"} 0"#),
        "missing api_v1 zero series in:\n{out}"
    );
}
