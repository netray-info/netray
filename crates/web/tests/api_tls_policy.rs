//! API v1 TLS inspect target policy (spec v2-api, Phase 5, C7 and C8): `POST /tls/inspect` through
//! `netray_web::api::router` over the TLS stub parts; a blocked target never reaches a handshake.
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_tls::testing::{StubTlsBackend, stub_state};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, router,
};
use serde_json::Value;
use tower::ServiceExt;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: std::net::IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> futures::future::BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn app(backend: Arc<StubTlsBackend>) -> Router {
    let parts = ApiParts {
        tls: Some(stub_state(backend).inspector()),
        ..Default::default()
    };
    router(ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(parts))
}

async fn post(app: Router, body: &str) -> (StatusCode, Value) {
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
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("{body}: body is not JSON: {e}: {bytes:?}"));
    (status, json)
}

/// C7, C8: a private literal and a name resolving to a private address are both refused with
/// 403 `TARGET_BLOCKED` before any handshake.
#[tokio::test]
async fn blocked_targets_answer_403_without_a_handshake() {
    let cases = [
        (
            r#"{"host":"10.0.0.1"}"#,
            StubTlsBackend::new(),
            "literal private address",
        ),
        (
            r#"{"host":"internal.example.com"}"#,
            StubTlsBackend::new().with_host("internal.example.com", vec!["10.0.0.5".parse().unwrap()]),
            "name resolving to a private address",
        ),
    ];
    for (body, stub, label) in cases {
        let backend = Arc::new(stub);
        let (status, json) = post(app(backend.clone()), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{label}: {json}");
        assert_eq!(json["error"]["code"], "TARGET_BLOCKED", "{label}: {json}");
        assert!(
            json["error"]["message"].as_str().is_some_and(|m| !m.is_empty()),
            "{label}: {json}"
        );
        assert!(backend.calls().is_empty(), "{label}: a handshake was made");
    }
}
