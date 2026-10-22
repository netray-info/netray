// V2 Phase 5, requirements 28 and 31 (first bullet): the HTTP inspection as a function with no
// axum type, over `testing::StubHttpInspect` (offline). Needs `required-features = ["testing"]` on this test target.

use std::net::{IpAddr, Ipv4Addr};

use netray_http::job::{self, HttpInspectRequest, HttpRefusal};
use netray_http::security::RateLimitState;
use netray_http::security::rate_limit::{Quotas, Scopes};
use netray_http::testing::StubHttpInspect;
use serde_json::Value;

const SPECTRA_JSON: &str = include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

fn caller() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1))
}

fn limits() -> RateLimitState {
    RateLimitState::scoped(
        &Quotas {
            per_ip_per_minute: 10,
            per_ip_burst: 5,
            per_target_per_minute: 30,
            per_target_burst: 10,
        },
        Scopes {
            metrics_prefix: "spectra",
            per_ip: "per_ip",
            per_target: "per_target",
        },
    )
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
