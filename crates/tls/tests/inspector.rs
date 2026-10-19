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

// ---------------------------------------------------------------------------
// v2-input-edges Phase 2 (req. 8; C3, C11, C12, C13): a cost above the per-IP burst is an
// input error, never a rate limit
// ---------------------------------------------------------------------------

mod cost_limit {
    use super::*;
    use axum::response::IntoResponse;
    use netray_tls::error::AppError;
    use netray_tls::security::rate_limit::{Quotas, Scopes};

    fn limiter(per_ip_burst: u32, per_target_burst: u32) -> RateLimitState {
        RateLimitState::scoped(
            &Quotas {
                per_ip_per_minute: per_ip_burst,
                per_ip_burst,
                per_target_per_minute: 20,
                per_target_burst,
            },
            Scopes {
                metrics_prefix: "tlsight",
                per_ip: "per_ip",
                per_target: "per_target",
            },
        )
    }

    #[test]
    fn c11_cost_above_the_per_ip_burst_is_refused_and_charges_nothing() {
        let limits = limiter(2, 20);
        let err = limits.check_cost(caller(), "example.com", 3).unwrap_err();
        assert!(
            matches!(err, AppError::CostExceedsLimit { cost: 3, limit: 2 }),
            "{err:?}"
        );
        assert!(limits.check_cost(caller(), "example.com", 2).is_ok());
    }

    #[test]
    fn c12_remaining_budget_is_capped_by_the_per_target_burst() {
        assert_eq!(limiter(10, 4).remaining_budget(caller()), 4);
    }

    #[tokio::test]
    async fn c13_cost_exceeds_limit_answers_422_without_retry_after() {
        let resp = AppError::CostExceedsLimit { cost: 3, limit: 2 }.into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(!resp.headers().contains_key("retry-after"));
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "COST_EXCEEDS_LIMIT");
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains('3') && message.contains('2'), "{message}");

        let refusal = TlsRefusal::from(AppError::CostExceedsLimit { cost: 3, limit: 2 });
        assert!(
            matches!(refusal, TlsRefusal::InvalidInput(_)),
            "{refusal:?}"
        );
    }

    #[tokio::test]
    async fn c3_three_ports_over_a_burst_of_two_is_invalid_input_not_rate_limited() {
        let inspector = stub_state(Arc::new(StubTlsBackend::new())).inspector();
        let request = TlsInspectRequest {
            host: "example.com".into(),
            port: None,
            ports: Some(vec![443, 8443, 993]),
        };
        let err = inspector
            .inspect(&request, caller(), &limiter(2, 20), "rid".into())
            .await
            .unwrap_err();
        match err {
            TlsRefusal::InvalidInput(message) => {
                assert!(message.contains('3') && message.contains('2'), "{message}");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }
}
