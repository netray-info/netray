//! v2-inspectors requirement 14 and SDD §6.2 on the apex composition: the inspector pages and
//! `/api/v1/<tool>` draw on one set of limiters, as lens's composition did. A per-client budget
//! spent through one answers 429 on the other.
mod common;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use netray_engine::Registry;
use netray_ip::testing::StubIpLookup;
use netray_web::api::ApiConfig;
use netray_web::config::ApexConfig;
use tower::ServiceExt;

const CLIENT: &str = "192.0.2.7";

/// The apex with `[api.ip] per_ip_burst = 2` over a stub lookup: no network.
fn app() -> Router {
    let api: ApiConfig =
        toml::from_str("[ip]\ninternal_mode = true\nper_ip_burst = 2\nper_ip_per_minute = 1\n")
            .expect("[api.ip] keys parse");
    let config = ApexConfig {
        api,
        ..ApexConfig::default()
    };
    let registry = Registry::new();
    let plan = common::default_plan(&registry);
    let mut state = common::state_with(config, registry, plan);
    state.api_parts.ip = Some(Arc::new(StubIpLookup::new()));
    netray_web::host::apex::app(state)
}

async fn send(app: &Router, mut req: Request<Body>) -> (StatusCode, String) {
    let peer: SocketAddr = format!("{CLIENT}:40000").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn v1(app: &Router) -> (StatusCode, String) {
    send(
        app,
        Request::get("/api/v1/ip/192.0.2.1")
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

async fn page(app: &Router) -> (StatusCode, String) {
    send(
        app,
        Request::post("/inspect/ip/run")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("ip=192.0.2.2"))
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn ip_budget_spent_on_api_v1_refuses_the_inspector_page() {
    let app = app();
    for i in 0..2 {
        let (status, body) = v1(&app).await;
        assert_eq!(status, StatusCode::OK, "v1 call {i}: {body}");
    }
    let (status, body) = page(&app).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "page: {body}");
    assert!(body.contains("too many requests"), "page: {body}");
}

#[tokio::test]
async fn ip_budget_spent_on_the_inspector_page_refuses_api_v1() {
    let app = app();
    for i in 0..2 {
        let (status, body) = page(&app).await;
        assert_eq!(status, StatusCode::OK, "page run {i}: {body}");
    }
    let (status, body) = v1(&app).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "v1: {body}");
}
