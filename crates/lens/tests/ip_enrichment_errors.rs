//! Enrichment failures must not score as healthy (spec grade-integrity; criteria C5, C6).
//!
//! A failed or timed-out ifconfig-rs enrichment call makes the IP section Errored
//! (`Err(SectionError::BackendError | Timeout)`), which is what makes the overall result
//! incomplete. Asserted through `IpBackend::run` and `check_ip`; the scoring engine is not driven.

use std::net::IpAddr;
use std::time::Duration;

use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use lens::backends::ip::{IpBackend, check_ip};
use lens::backends::{Backend, BackendContext};
use lens::check::SectionError;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("../../../tests/fixtures/contracts/ifconfig-json.json");
const HEALTHY: &str = "203.0.113.42";
const OTHER: &str = "198.51.100.7";

fn public_or_documentation(ip: IpAddr) -> bool {
    match netray_common::target_policy::refusal_reason(ip) {
        None => true,
        Some(r) => r.starts_with("documentation"),
    }
}

async fn serve(app: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://{addr}")
}

async fn all_500() -> String {
    serve(axum::Router::new().route("/json", get(|| async { StatusCode::INTERNAL_SERVER_ERROR })))
        .await
}

/// `HEALTHY` gets the golden at once; every other address sleeps past any timeout.
async fn one_stalls() -> String {
    serve(axum::Router::new().route(
        "/json",
        get(|Query(q): Query<HashMap<String, String>>| async move {
            if q.get("ip").map(String::as_str) == Some(HEALTHY) {
                ([(header::CONTENT_TYPE, "application/json")], GOLDEN).into_response()
            } else {
                tokio::time::sleep(Duration::from_secs(30)).await;
                StatusCode::OK.into_response()
            }
        }),
    ))
    .await
}

fn backend(url: String, timeout: Duration) -> IpBackend {
    IpBackend {
        ip_url: url.clone(),
        public_url: url,
        timeout,
        client: reqwest::Client::new(),
        allow: public_or_documentation,
    }
}

fn context(ips: &[&str]) -> BackendContext {
    BackendContext {
        resolved_ips: ips.iter().map(|s| s.parse().unwrap()).collect(),
        dkim_selectors: None,
        forward_headers: Default::default(),
    }
}

fn is_errored<T>(r: &Result<T, SectionError>) -> bool {
    matches!(
        r,
        Err(SectionError::BackendError(_) | SectionError::Timeout)
    )
}

#[tokio::test]
async fn c5_every_enrichment_answering_500_errors_the_ip_section() {
    let url = all_500().await;
    let ips: Vec<IpAddr> = vec![HEALTHY.parse().unwrap(), OTHER.parse().unwrap()];

    let direct = check_ip(
        &reqwest::Client::new(),
        &url,
        &ips,
        Duration::from_secs(5),
        &Default::default(),
        public_or_documentation,
    )
    .await;
    assert!(
        direct.is_err(),
        "check_ip must be Err when every enrichment fails"
    );

    let run = backend(url, Duration::from_secs(5))
        .run("example.com", &context(&[HEALTHY, OTHER]))
        .await;
    assert!(is_errored(&run), "IP section must be Errored, got {run:?}");
}

#[tokio::test]
async fn c6_one_timed_out_enrichment_errors_the_ip_section() {
    let url = one_stalls().await;
    let timeout = Duration::from_millis(300);
    let ips: Vec<IpAddr> = vec![HEALTHY.parse().unwrap(), OTHER.parse().unwrap()];

    let direct = check_ip(
        &reqwest::Client::new(),
        &url,
        &ips,
        timeout,
        &Default::default(),
        public_or_documentation,
    )
    .await;
    assert!(
        direct.is_err(),
        "check_ip must be Err when one enrichment times out"
    );

    let run = backend(url, timeout)
        .run("example.com", &context(&[HEALTHY, OTHER]))
        .await;
    assert!(is_errored(&run), "IP section must be Errored, got {run:?}");
}
