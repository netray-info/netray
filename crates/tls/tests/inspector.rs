// V2 Phase 5, requirements 27, 28, 31 (C5 crate half, C12, C14): the TLS inspection type called
// directly with a stub backend and no axum type, and tlsight's POST /api/inspect unchanged over
// the same stub. Needs `required-features = ["testing"]` on this test target.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_engine::SectionOutcome;
use netray_model::evidence::Block;
use netray_tls::config::Config;
use netray_tls::routes::api_router;
use netray_tls::security::RateLimitState;
use netray_tls::testing::{StubTlsBackend, golden_module, stub_config, stub_state};
use netray_tls::{TlsInspectRequest, TlsRefusal, translate};
use serde_json::Value;
use tower::ServiceExt;

const GOLDEN: &str = include_str!("../../../tests/fixtures/contracts/tlsight-inspect.json");

fn caller() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1))
}

fn limits() -> RateLimitState {
    let config: Config = stub_config();
    RateLimitState::new(&config.limits)
}

fn req(host: &str, port: u16) -> TlsInspectRequest {
    TlsInspectRequest {
        host: host.into(),
        port: Some(port),
        ports: None,
    }
}

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

#[tokio::test]
async fn example_com_inspects_port_443() {
    let stub = Arc::new(StubTlsBackend::new());
    let inspector = stub_state(stub.clone()).inspector();
    let resp = inspector
        .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
        .await
        .expect("inspection answers");
    assert_eq!(resp.hostname, "example.com");
    assert_eq!(resp.ports.len(), 1);
    assert_eq!(resp.ports[0].port, 443);
    assert_eq!(
        stub.calls(),
        vec![(ip("93.184.215.14"), 443, Some("example.com".to_string()))]
    );
}

#[tokio::test]
async fn mail_host_on_port_25_runs_smtp_starttls() {
    let stub = Arc::new(StubTlsBackend::new());
    let inspector = stub_state(stub.clone()).inspector();
    let resp = inspector
        .inspect(
            &req("mail.example.com", 25),
            caller(),
            &limits(),
            "rid".into(),
        )
        .await
        .expect("inspection answers");
    assert_eq!(
        stub.calls(),
        vec![(
            ip("93.184.215.14"),
            25,
            Some("mail.example.com".to_string())
        )]
    );
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["ports"][0]["port"], 25);
    assert_eq!(v["ports"][0]["ips"][0]["tls"]["starttls"], "smtp");
}

#[tokio::test]
async fn result_translates_to_measured_tls_checks_with_evidence() {
    let inspector = stub_state(Arc::new(StubTlsBackend::new())).inspector();
    let resp = inspector
        .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
        .await
        .unwrap();
    let (checks, evidence): (_, Vec<Block>) = match translate(&resp) {
        SectionOutcome::Measured { checks, evidence } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    };
    let checks: Vec<netray_model::CheckResult> = checks;
    let got: Vec<String> = checks.iter().map(|c| c.id.to_string()).collect();
    let expected: Vec<String> = golden_module(GOLDEN)
        .checks()
        .iter()
        .map(|c| c.to_string())
        .collect();
    assert!(!expected.is_empty());
    assert!(expected.iter().all(|id| id.starts_with("tls.")));
    for id in &expected {
        assert!(got.contains(id), "missing check {id}; got {got:?}");
    }
    assert!(!evidence.is_empty(), "at least one evidence block");
}

#[tokio::test]
async fn localhost_is_invalid_input() {
    let inspector = stub_state(Arc::new(StubTlsBackend::new())).inspector();
    let err = inspector
        .inspect(&req("localhost", 443), caller(), &limits(), "rid".into())
        .await
        .unwrap_err();
    assert!(matches!(err, TlsRefusal::InvalidInput(_)), "{err:?}");
}

#[tokio::test]
async fn private_literal_and_private_resolution_are_blocked() {
    let stub =
        Arc::new(StubTlsBackend::new().with_host("internal.example.com", vec![ip("10.0.0.5")]));
    let inspector = stub_state(stub.clone()).inspector();
    for host in ["10.0.0.1", "internal.example.com"] {
        let err = inspector
            .inspect(&req(host, 443), caller(), &limits(), "rid".into())
            .await
            .unwrap_err();
        assert!(matches!(err, TlsRefusal::Blocked(_)), "{host}: {err:?}");
    }
    assert!(stub.calls().is_empty(), "no handshake to a blocked address");
}

// ---------------------------------------------------------------------------
// C12: tlsight's POST /api/inspect is unchanged
// ---------------------------------------------------------------------------

fn post(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/inspect")
        .header("content-type", "application/json")
        .extension(ConnectInfo::<SocketAddr>(SocketAddr::new(caller(), 12345)))
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn router(stub: Arc<StubTlsBackend>) -> axum::Router {
    api_router(stub_state(stub)).layer(axum::middleware::from_fn(
        netray_common::middleware::request_id,
    ))
}

async fn answer(router: axum::Router, body: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
    let resp = router.oneshot(post(body)).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, headers, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn tlsight_post_answers_what_the_inspector_answers() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let stub = Arc::new(StubTlsBackend::new());
    let (status, headers, mut body) =
        answer(router(stub.clone()), r#"{"hostname":"example.com"}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.contains_key("x-cert-expiry"));

    let direct = stub_state(stub)
        .inspector()
        .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
        .await
        .unwrap();
    let mut direct = serde_json::to_value(&direct).unwrap();
    for v in [&mut body, &mut direct] {
        let o = v.as_object_mut().unwrap();
        o.remove("request_id");
        o.remove("duration_ms");
    }
    assert_eq!(body, direct);
}

#[tokio::test]
async fn tlsight_post_refusals_keep_their_codes() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let stub = Arc::new(StubTlsBackend::new());
    let (status, _, body) = answer(router(stub.clone()), r#"{"hostname":"localhost"}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "INVALID_HOSTNAME");

    let (status, _, body) = answer(router(stub), r#"{"hostname":"10.0.0.1"}"#).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "BLOCKED_TARGET");
}
