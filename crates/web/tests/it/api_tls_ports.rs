//! V2 spec v2-tls-backend, Phase 2 (R11): `POST /tls/inspect` with `ports`, driven through
//! `netray_web::api::router` over a stub handshake backend.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_tls::testing::{StubTlsBackend, stub_state};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, openapi, router,
};
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
    ) -> futures::future::BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn app(parts: ApiParts) -> Router {
    router(ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(parts))
}

fn stub_app(stub: &Arc<StubTlsBackend>) -> Router {
    app(ApiParts {
        tls: Some(stub_state(stub.clone()).inspector()),
        ..Default::default()
    })
}

async fn post(app: Router, body: Value) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/tls/inspect")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let parsed = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("body is not JSON: {e}: {bytes:?}"));
    (status, parsed)
}

fn port_numbers(body: &Value) -> Vec<i64> {
    body["ports"]
        .as_array()
        .unwrap_or_else(|| panic!("no ports array: {body}"))
        .iter()
        .map(|p| p["port"].as_i64().unwrap())
        .collect()
}

/// C16: `ports` inspects each port, in the order given.
#[tokio::test]
async fn ports_inspects_each_port_in_order() {
    let stub = Arc::new(StubTlsBackend::new());
    let (status, body) = post(
        stub_app(&stub),
        json!({"host": "example.com", "ports": [443, 465]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(port_numbers(&body), vec![443, 465], "{body}");
}

/// C17: malformed `ports` input answers 400 with the API error body.
#[tokio::test]
async fn malformed_ports_answer_bad_request() {
    let cases = [
        json!({"host": "example.com", "port": 443, "ports": [465]}),
        json!({"host": "example.com", "ports": []}),
        json!({"host": "example.com", "ports": [1, 2, 3, 4, 5, 6, 7, 8]}),
        json!({"host": "example.com", "ports": [0]}),
    ];
    for case in cases {
        let stub = Arc::new(StubTlsBackend::new());
        let (status, body) = post(stub_app(&stub), case.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{case}: {body}");
        assert!(body["error"].is_object(), "{case}: {body}");
    }
}

/// C18: without `ports`, `port` (default 443) still yields exactly one result.
#[tokio::test]
async fn port_alone_yields_one_result() {
    let cases = [
        (json!({"host": "example.com"}), 443),
        (json!({"host": "example.com", "port": 25}), 25),
    ];
    for (case, expected) in cases {
        let stub = Arc::new(StubTlsBackend::new());
        let (status, body) = post(stub_app(&stub), case.clone()).await;
        assert_eq!(status, StatusCode::OK, "{case}: {body}");
        assert_eq!(port_numbers(&body), vec![expected], "{case}: {body}");
    }
}

/// C19: the document describes `ports` on the request schema as an array.
#[test]
fn openapi_documents_ports_as_array() {
    let doc = serde_json::to_value(openapi()).unwrap();
    let ports = &doc["components"]["schemas"]["TlsInspectRequest"]["properties"]["ports"];
    assert_eq!(ports["type"], "array", "{ports}");
}
