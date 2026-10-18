// V2 Phase 5, requirements 28 and 31 (first bullet): the HTTP inspection as a function with no
// axum type, over `testing::StubHttpInspect` (offline), and spectra's own `POST /api/inspect`
// unchanged. Needs `required-features = ["testing"]` on this test target.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use netray_http::config::Config;
use netray_http::job::{self, HttpInspectRequest, HttpRefusal};
use netray_http::routes::api_router;
use netray_http::security::RateLimitState;
use netray_http::state::AppState;
use netray_http::testing::StubHttpInspect;
use serde_json::Value;
use tower::ServiceExt;

const SPECTRA_JSON: &str = include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

fn caller() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1))
}

fn limits() -> RateLimitState {
    RateLimitState::new(&Config::load(None).unwrap().limits)
}

fn req(url: &str) -> HttpInspectRequest {
    HttpInspectRequest {
        url: url.to_string(),
    }
}

#[tokio::test]
async fn inspect_answers_the_fixture_body_for_an_allowed_url() {
    let resp = job::inspect(
        &req("https://example.com/"),
        caller(),
        &limits(),
        &StubHttpInspect::new(),
    )
    .await
    .expect("allowed target is inspected");

    let mut want: Value = serde_json::from_str(SPECTRA_JSON).unwrap();
    want["url"] = Value::String("https://example.com/".to_string());
    assert_eq!(serde_json::to_value(&resp).unwrap(), want);
}

#[tokio::test]
async fn inspect_refuses_a_reserved_address_as_blocked() {
    let got = job::inspect(
        &req("https://127.0.0.1/"),
        caller(),
        &limits(),
        &StubHttpInspect::new(),
    )
    .await
    .unwrap_err();
    assert!(matches!(got, HttpRefusal::Blocked(_)), "{got:?}");
}

#[tokio::test]
async fn inspect_refuses_the_http_scheme_with_parse_urls_text() {
    let got = job::inspect(
        &req("http://example.com/"),
        caller(),
        &limits(),
        &StubHttpInspect::new(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        got,
        HttpRefusal::InvalidInput(
            "invalid URL: Submit the URL with https:// scheme; port-80 probing is automatic."
                .to_string()
        )
    );
}

#[tokio::test]
async fn inspect_returns_rate_limited_once_the_per_ip_budget_is_spent() {
    let limits = limits();
    for n in 0..5 {
        limits
            .check(caller(), &format!("host{n}.example.com"))
            .expect("within the default per-IP burst of 5");
    }

    let got = job::inspect(
        &req("https://example.com/"),
        caller(),
        &limits,
        &StubHttpInspect::new(),
    )
    .await
    .unwrap_err();
    match got {
        HttpRefusal::RateLimited { retry_after_secs } => assert!(retry_after_secs > 0),
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

async fn spectra_post(url: &str) -> (StatusCode, Value) {
    let state = AppState::new(&Config::load(None).unwrap());
    let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)), 12345);
    let app = api_router(state).layer(MockConnectInfo(peer));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inspect")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({ "url": url }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test]
async fn spectra_post_inspect_keeps_its_codes() {
    for (url, status, code) in [
        (
            "http://example.com/",
            StatusCode::BAD_REQUEST,
            "INVALID_URL",
        ),
        (
            "https://127.0.0.1/",
            StatusCode::FORBIDDEN,
            "TARGET_BLOCKED",
        ),
    ] {
        let (got, body) = spectra_post(url).await;
        assert_eq!(got, status, "{url}");
        assert_eq!(body["error"]["code"], code, "{url}");
    }
}
