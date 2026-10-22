//! spec v2-input-limits, Phase 2 (R4): API v1's DNS query counts the DNSKEY and DS types
//! `+dnssec` adds before the record-type limit, so ten types plus `+dnssec` get the refusal an
//! eleventh type gets, on `POST /dns/query` and `GET /dns/query?q=` alike.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger,
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
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

const PROXY: &str = "203.0.113.9";
const CALLER: &str = "198.51.100.1";
const TEN: [&str; 10] = [
    "A", "AAAA", "MX", "TXT", "NS", "SOA", "CAA", "CNAME", "PTR", "SRV",
];

fn app() -> Router {
    let parts = ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        ..Default::default()
    };
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns))
        .with_ip_extractor(Arc::new(IpExtractor::new(&[PROXY.to_string()])))
        .with_parts(parts);
    netray_web::api::router(state)
}

async fn send(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-forwarded-for", CALLER)
        .header("accept", "application/json");
    let body = match body {
        Some(b) => {
            builder = builder.header("content-type", "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let mut req = builder.body(body).unwrap();
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// The refusal an eleventh record type gets today.
async fn eleventh_type_refusal(app: &Router) -> Value {
    let mut types = TEN.to_vec();
    types.push("HTTPS");
    let (status, body) = send(
        app,
        "POST",
        "/dns/query",
        Some(json!({"domain": "example.com", "record_types": types})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    body["error"]["code"].clone()
}

#[tokio::test]
async fn post_ten_types_plus_dnssec_get_the_eleventh_type_refusal() {
    let app = app();
    let want = eleventh_type_refusal(&app).await;
    let (status, body) = send(
        &app,
        "POST",
        "/dns/query",
        Some(json!({"domain": "example.com", "record_types": TEN, "dnssec": true})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], want, "{body}");
}

#[tokio::test]
async fn get_ten_types_plus_dnssec_get_the_eleventh_type_refusal() {
    let app = app();
    let want = eleventh_type_refusal(&app).await;
    let (status, body) = send(
        &app,
        "GET",
        "/dns/query?q=example.com%20A%20AAAA%20MX%20TXT%20NS%20SOA%20CAA%20CNAME%20PTR%20SRV%20%2Bdnssec",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], want, "{body}");
}
