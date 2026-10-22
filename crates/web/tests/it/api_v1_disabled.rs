//! `[api] enabled = false` leaves every `/api/v1` path to lens as before (spec v2-api, requirement 8):
//! no JSON 413 and no relaxed docs CSP from v1.
//! Copied from crates/lens/tests/api_v1_disabled.rs (spec v2-cutover, Phase 6); nothing dropped
//! (an unmatched path answers the pages' 404 now, not the SPA).
use crate::common;

use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use netray_engine::Registry;
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

fn app(config: ApexConfig) -> Router {
    let registry = Registry::new();
    let plan = common::default_plan(&registry);
    netray_web::host::apex::app(common::state_with(config, registry, plan))
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
async fn disabled_api_answers_no_json_413_and_no_relaxed_csp_under_v1() {
    let mut config = base_config();
    config.api.enabled = false;
    let app = app(config);
    let req = Request::get("/api/v1/catalogue")
        .header("content-length", "9000")
        .body(Body::from(vec![b'a'; 9000]))
        .unwrap();
    let (s, ct, body) = send(&app, req).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!is_json(&ct), "v1 is off, yet a JSON 413: {ct:?} {body}");

    let csp = |path: &'static str| {
        let app = app.clone();
        async move {
            let mut req = Request::get(path).body(Body::empty()).unwrap();
            req.extensions_mut().insert(ConnectInfo(
                "203.0.113.9:4000".parse::<SocketAddr>().unwrap(),
            ));
            let resp = app.oneshot(req).await.unwrap();
            resp.headers()
                .get("content-security-policy")
                .map(|v| v.to_str().unwrap().to_string())
        }
    };
    assert_eq!(
        csp("/api/v1/docs").await,
        csp("/nope").await,
        "v1 is off: /api/v1/docs is an unmatched path and keeps the strict CSP"
    );
}
