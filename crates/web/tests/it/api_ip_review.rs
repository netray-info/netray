//! spec v2-ip-cli, Phase 1 review: a module-off caller is never charged for a 503 (requirement 8),
//! structured `headers` keeps V1's `x_forwarded_for_chain`, and `?ip=` answers `cidr` and
//! `headers` with the IP module off (requirements 8 and 15).
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::testing::StubIpLookup;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const CALLER: &str = "192.0.2.10:40000";

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: std::net::IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn app(module_on: bool) -> Router {
    let parts = if module_on {
        ApiParts {
            ip: Some(Arc::new(StubIpLookup::new())),
            ..Default::default()
        }
    } else {
        ApiParts::default()
    };
    netray_web::api::router(ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(parts))
}

async fn get(app: &Router, path: &str, headers: &[(&str, &str)]) -> (StatusCode, String) {
    let mut req = Request::get(path);
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let mut req = req.body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(CALLER.parse::<SocketAddr>().unwrap()));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn a_module_off_caller_gets_503_past_the_per_ip_burst_never_429() {
    let app = app(false);
    let burst = ApiConfig::default().ip.per_ip_burst;
    for n in 0..burst + 2 {
        let (status, body) = get(&app, "/ip?field=country", &[]).await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "request {n}: {body}"
        );
    }
    let (status, body) = get(&app, "/ip?field=ip", &[]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn structured_headers_carry_the_forwarded_for_chain() {
    let app = app(true);
    let (status, body) = get(
        &app,
        "/ip?field=headers&format=json",
        &[("x-forwarded-for", "203.0.113.9, 198.51.100.7")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        v["x_forwarded_for_chain"],
        json!(["203.0.113.9", "198.51.100.7"]),
        "{body}"
    );
    let (_, body) = get(&app, "/ip?field=headers&format=json", &[]).await;
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["x_forwarded_for_chain"], json!([]), "{body}");
}

#[tokio::test]
async fn ip_query_answers_cidr_and_headers_with_the_module_off() {
    let app = app(false);
    let (status, body) = get(&app, "/ip?ip=198.51.100.1&field=cidr", &[]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, "198.51.100.1/32\n");
    let (status, body) = get(
        &app,
        "/ip?ip=198.51.100.1&field=headers",
        &[("x-test", "one")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("x-test: one"), "{body}");
    let (status, _) = get(&app, "/ip?ip=198.51.100.1&field=country", &[]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
