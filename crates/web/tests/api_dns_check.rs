//! API v1 `/dns/check` (spec v2-dns-api, Phase 5, requirement 15): the DNS health check runs the
//! DNS module alone and answers JSON, driven through `netray_web::api::router` over the golden
//! DNS module. Paths are relative to the mount.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
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
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn golden() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/contracts/prism.sse"
    ))
    .expect("prism check fixture")
}

fn app_with(config: ApiConfig, with_module: bool) -> Router {
    let parts = ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        dns_module: with_module.then(|| Arc::from(netray_dns::testing::golden_module(&golden()))),
        ..Default::default()
    };
    router(ApiState::new(config, Arc::new(NoRuns)).with_parts(parts))
}

struct Reply {
    status: StatusCode,
    content_type: String,
    headers: axum::http::HeaderMap,
    body: String,
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    accept: Option<&str>,
    body: Option<Value>,
) -> Reply {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(a) = accept {
        builder = builder.header("accept", a);
    }
    let body = match body {
        Some(b) => {
            builder = builder.header("content-type", "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let mut req = builder.body(body).unwrap();
    let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        content_type,
        headers,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

fn json_of(r: &Reply) -> Value {
    serde_json::from_str(&r.body).unwrap_or_else(|e| panic!("body is not JSON: {e}: {}", r.body))
}

fn validate(schema: Value, value: &Value) {
    let doc = serde_json::to_value(openapi()).unwrap();
    let mut root = schema;
    assert!(root.is_object(), "schema not documented");
    root.as_object_mut()
        .unwrap()
        .insert("components".into(), doc["components"].clone());
    let validator = jsonschema::validator_for(&root).expect("schema compiles");
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}\nvalue: {value}");
}

fn validate_op(method: &str, body: &Value) {
    let doc = serde_json::to_value(openapi()).unwrap();
    let schema = doc["paths"]["/api/v1/dns/check"][method]["responses"]["200"]["content"]
        ["application/json"]["schema"]
        .clone();
    validate(schema, body);
}

/// The shared shape of a 200 answer; returns the body.
async fn assert_check_shape(r: &Reply, method: &str) -> Value {
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("application/json"),
        "{}",
        r.content_type
    );
    let v = json_of(r);
    assert_eq!(v["protocol"], "dns");
    assert_eq!(v["outcome"], "measured");
    assert!(v["reason"].is_null());
    let checks = v["checks"].as_array().expect("checks array");
    assert_eq!(checks.len(), 10, "the ten dns checks: {checks:?}");
    for c in checks {
        assert!(c["id"].as_str().unwrap().starts_with("dns."), "{c}");
        assert!(c["status"].is_string(), "{c}");
        assert!(c["findings"].is_array(), "{c}");
        assert!(c["evidence"].is_array(), "{c}");
    }
    assert!(v["blocks"].is_array());
    let key = v["cache_key"].as_str().expect("cache_key");
    assert_eq!(key.len(), 12, "{key}");
    assert!(
        key.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
        "{key}"
    );
    validate_op(method, &v);
    v
}

async fn assert_replays_as_check(app: &Router, key: &str) {
    let r = send(app, "GET", &format!("/dns/results/{key}"), None, None).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(json_of(&r)["mode"], "check");
}

/// C1, C2: POST answers the module's ten checks, blocks and a cache_key that replays as check.
#[tokio::test]
async fn post_dns_check_answers_the_ten_checks_and_replays() {
    let app = app_with(ApiConfig::default(), true);
    let r = send(
        &app,
        "POST",
        "/dns/check",
        None,
        Some(json!({"domain": "example.com"})),
    )
    .await;
    let v = assert_check_shape(&r, "post").await;
    assert_replays_as_check(&app, v["cache_key"].as_str().unwrap()).await;
    let doc = serde_json::to_value(openapi()).unwrap();
    assert!(doc["components"]["schemas"]["DnsCheckResult"].is_object());
}

/// C1, C3: GET ?q= answers the same shape, the domain being q's first token.
#[tokio::test]
async fn get_dns_check_answers_the_same_shape() {
    let app = app_with(ApiConfig::default(), true);
    let r = send(&app, "GET", "/dns/check?q=example.com", None, None).await;
    let v = assert_check_shape(&r, "get").await;
    assert_replays_as_check(&app, v["cache_key"].as_str().unwrap()).await;
}

/// C1, C4: an IP address and an empty domain are 400 INVALID_INPUT.
#[tokio::test]
async fn an_ip_or_empty_domain_is_400_invalid_input() {
    let app = app_with(ApiConfig::default(), true);
    for domain in ["192.0.2.1", ""] {
        let r = send(
            &app,
            "POST",
            "/dns/check",
            None,
            Some(json!({"domain": domain})),
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{domain:?}: {}", r.body);
        assert_eq!(json_of(&r)["error"]["code"], "INVALID_INPUT", "{domain:?}");
    }
}

/// C1, C5: without a DNS module the answer is 503 UNAVAILABLE.
#[tokio::test]
async fn no_dns_module_is_503_unavailable() {
    let app = app_with(ApiConfig::default(), false);
    let r = send(
        &app,
        "POST",
        "/dns/check",
        None,
        Some(json!({"domain": "example.com"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE, "{}", r.body);
    assert_eq!(json_of(&r)["error"]["code"], "UNAVAILABLE");
}

/// C1, C6: a flat cost of 16 over a burst of 10 is refused with 429 and Retry-After.
#[tokio::test]
async fn a_caller_over_the_dns_budget_is_429() {
    let config: ApiConfig = toml::from_str("[dns]\nper_ip_burst = 10").expect("[api.dns] parses");
    let app = app_with(config, true);
    let r = send(
        &app,
        "POST",
        "/dns/check",
        None,
        Some(json!({"domain": "example.com"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.body);
    let retry: u64 = r.headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(retry >= 1);
}

/// C1, C7: Accept text/event-stream still answers JSON, on both methods.
#[tokio::test]
async fn event_stream_accept_still_answers_json() {
    let app = app_with(ApiConfig::default(), true);
    let sse = Some("text/event-stream");
    let post = send(
        &app,
        "POST",
        "/dns/check",
        sse,
        Some(json!({"domain": "example.com"})),
    )
    .await;
    assert_check_shape(&post, "post").await;
    let get = send(&app, "GET", "/dns/check?q=example.com", sse, None).await;
    assert_check_shape(&get, "get").await;
}

/// Phase 5 review: the check runs on exactly the name asked: an address with a trailing dot, a
/// name with a space or a flag, and an over-long name are refused, not run on something else.
#[tokio::test]
async fn names_the_check_cannot_run_on_are_400() {
    let app = app_with(ApiConfig::default(), true);
    let long = "a".repeat(254);
    for domain in [
        "192.0.2.1.",
        "example.com other",
        "example.com +dnssec",
        "@google",
        long.as_str(),
    ] {
        let r = send(
            &app,
            "POST",
            "/dns/check",
            None,
            Some(json!({"domain": domain})),
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{domain:?}: {}", r.body);
        assert_eq!(json_of(&r)["error"]["code"], "INVALID_INPUT", "{domain:?}");
    }
    let r = send(&app, "GET", "/dns/check?q=192.0.2.1.", None, None).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.body);
}
