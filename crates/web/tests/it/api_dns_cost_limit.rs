//! spec v2-input-edges, Phase 2 (R6, R7, R9): API v1's DNS routes refuse a query whose cost
//! exceeds a burst with 400 INVALID_INPUT and no `Retry-After` (a wait cannot admit it), while a
//! request within every burst that finds too few tokens still answers 429 RATE_LIMITED.
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
const SERVERS: [&str; 3] = ["cloudflare", "google", "quad9"];

fn app(dns_table: &str) -> Router {
    let config: ApiConfig =
        toml::from_str(&format!("[dns]\n{dns_table}")).expect("[api.dns] parses");
    let parts = ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        ..Default::default()
    };
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(Arc::new(IpExtractor::new(&[PROXY.to_string()])))
        .with_parts(parts);
    netray_web::api::router(state)
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    json: Value,
}

async fn send(app: &Router, method: &str, uri: &str, body: Option<Value>) -> Reply {
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
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

/// 400 INVALID_INPUT whose message names `cost` and `limit`, with no `Retry-After`.
fn assert_cost_refusal(r: &Reply, cost: &str, limit: &str) {
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.json);
    assert_eq!(r.json["error"]["code"], "INVALID_INPUT", "{}", r.json);
    let message = r.json["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains(cost), "cost {cost} missing: {message}");
    assert!(message.contains(limit), "limit {limit} missing: {message}");
    assert!(!r.headers.contains_key("retry-after"), "{:?}", r.headers);
}

/// C8: with the default global burst 50, a compare of cost 60 is refused as invalid input.
#[tokio::test]
async fn compare_over_the_global_burst_answers_400_without_retry_after() {
    let app = app("");
    let r = send(
        &app,
        "POST",
        "/dns/compare",
        Some(json!({
            "domain": "example.com",
            "record_types": ["A", "AAAA", "MX"],
            "dnssec": true,
            "servers": SERVERS,
        })),
    )
    .await;
    assert_cost_refusal(&r, "60", "50");
}

/// C9: with global burst 20, a query of cost 30 is refused as invalid input, POST and GET alike.
#[tokio::test]
async fn query_over_the_global_burst_answers_400_on_post_and_get() {
    let app = app("global_burst = 20");
    let post = send(
        &app,
        "POST",
        "/dns/query",
        Some(json!({
            "domain": "example.com",
            "record_types": ["A", "AAAA", "MX", "TXT", "NS", "SOA", "CAA", "CNAME"],
            "dnssec": true,
            "servers": SERVERS,
        })),
    )
    .await;
    assert_cost_refusal(&post, "30", "20");
    let get = send(
        &app,
        "GET",
        "/dns/query?q=example.com%20A%20AAAA%20MX%20TXT%20NS%20SOA%20CAA%20CNAME%20%40cloudflare%20%40google%20%40quad9%20%2Bdnssec",
        None,
    )
    .await;
    assert_cost_refusal(&get, "30", "20");
}

/// C10 / C4: two queries of cost 18 within burst 20: the first runs, the second finds too few
/// tokens and is still rate limited with a `Retry-After`.
#[tokio::test]
async fn a_second_query_within_the_burst_that_finds_too_few_tokens_answers_429() {
    let app = app("global_burst = 20");
    let body = json!({
        "domain": "example.com",
        "record_types": ["A", "AAAA", "MX", "TXT", "NS", "SOA"],
        "servers": SERVERS,
    });
    let first = send(&app, "POST", "/dns/query", Some(body.clone())).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.json);
    let second = send(&app, "POST", "/dns/query", Some(body)).await;
    assert_eq!(
        second.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        second.json
    );
    assert_eq!(second.json["error"]["code"], "RATE_LIMITED");
    let retry: u64 = second.headers["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(retry >= 1);
}
