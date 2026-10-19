//! v2-input-edges, Phase 2, requirement 8 (C14, C15): a TLS request whose port-count cost exceeds
//! the `[api.tls]` per-IP burst is a 400 INVALID_INPUT on API v1 and a notice on the inspect page,
//! never a 429.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_tls::testing::{StubTlsBackend, stub_state};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use serde_json::{Value, json};
use tower::ServiceExt;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        unreachable!("no run is started by the TLS routes")
    }
}

fn state() -> ApiState {
    let config: ApiConfig = toml::from_str("[tls]\nper_ip_burst = 2").expect("[api.tls] parses");
    let inspector = stub_state(Arc::new(StubTlsBackend::new())).inspector();
    ApiState::new(config, Arc::new(NoRuns)).with_parts(ApiParts {
        tls: Some(inspector),
        ..Default::default()
    })
}

/// C14
#[tokio::test]
async fn api_cost_above_the_per_ip_burst_is_invalid_input_without_retry_after() {
    let app = netray_web::api::router(state());
    let mut req = Request::post("/tls/inspect")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"host": "example.com", "ports": [443, 8443, 993]}).to_string(),
        ))
        .unwrap();
    let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let retry_after = resp.headers().contains_key("retry-after");
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "INVALID_INPUT", "{body}");
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains('3') && message.contains('2'), "{body}");
    assert!(!retry_after, "no retry-after on a cost refusal");
}

/// C15
#[tokio::test]
async fn inspect_page_cost_above_the_per_ip_burst_is_a_notice_not_429() {
    let deps = InspectDeps::new(
        Arc::new(InspectJobs::new(state())),
        InspectConfig::default(),
    );
    let app: Router = inspect_router(deps);
    let resp = app
        .oneshot(
            Request::post("/inspect/tls/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("h=example.com%3A443%2C8443%2C993"))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let stream = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");

    assert_ne!(status, StatusCode::TOO_MANY_REQUESTS, "{stream}");
    assert!(stream.contains("cost 3"), "{stream}");
    assert!(stream.contains("exceeds the limit of 2"), "{stream}");
}
