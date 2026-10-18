//! API v1 TLS inspection (spec v2-api, Phase 5): `POST /tls/inspect` driven through
//! `netray_web::api::router` over a stub handshake backend, paths relative to the mount.
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

fn validate_200(body: &Value) {
    let doc = serde_json::to_value(openapi()).unwrap();
    let schema = &doc["paths"]["/api/v1/tls/inspect"]["post"]["responses"]["200"]["content"]["application/json"]
        ["schema"];
    assert!(schema.is_object(), "200 schema not documented");
    let mut root = schema.clone();
    root.as_object_mut()
        .unwrap()
        .insert("components".into(), doc["components"].clone());
    let validator = jsonschema::validator_for(&root).expect("200 schema compiles");
    let errors: Vec<String> = validator.iter_errors(body).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{errors:?}\nbody: {body}");
}

/// C4: a host answers the tlsight inspection for port 443, valid against the document.
#[tokio::test]
async fn host_is_inspected_on_443() {
    let stub = Arc::new(StubTlsBackend::new());
    let (status, body) = post(stub_app(&stub), json!({"host": "example.com"})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ports"].as_array().unwrap().len(), 1, "{body}");
    assert_eq!(body["ports"][0]["port"], 443, "{body}");
    validate_200(&body);
}

/// C5: port 25 inspects through SMTP STARTTLS.
#[tokio::test]
async fn port_25_uses_smtp_starttls() {
    let stub = Arc::new(StubTlsBackend::new());
    let (status, body) = post(
        stub_app(&stub),
        json!({"host": "mail.example.com", "port": 25}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ports"][0]["port"], 25, "{body}");
    assert_eq!(
        body["ports"][0]["ips"][0]["tls"]["starttls"], "smtp",
        "{body}"
    );
    let calls = stub.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].1, 25);
}

/// C6, C9: refused input answers 400 INVALID_INPUT.
#[tokio::test]
async fn refused_input_answers_invalid_input() {
    let cases = [
        json!({"host": "localhost", "port": 443}),
        json!({"host": "example.com", "starttls": "smtp"}),
    ];
    for case in cases {
        let stub = Arc::new(StubTlsBackend::new());
        let (status, body) = post(stub_app(&stub), case.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{case}: {body}");
        assert_eq!(body["error"]["code"], "INVALID_INPUT", "{case}: {body}");
    }
}

/// Without a TLS inspector the route answers 503 UNAVAILABLE.
#[tokio::test]
async fn no_inspector_answers_unavailable() {
    let (status, body) = post(app(ApiParts::default()), json!({"host": "example.com"})).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "UNAVAILABLE");
    assert_eq!(
        body["error"]["message"],
        "the TLS inspector is off: no TLS module was built"
    );
}
