//! API v1 GET forms of the DNS jobs (spec v2-dns-api, Phase 2, requirements 7, 8, 9): `q` is the
//! dig-like query language, driven through `netray_web::api::router` over the offline DNS stub,
//! paths relative to the mount.
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
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, openapi,
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

fn api_config(dns_table: &str) -> ApiConfig {
    toml::from_str(&format!("[dns]\n{dns_table}")).expect("[api.dns] keys parse")
}

fn setup(config: ApiConfig) -> (Router, ApiState) {
    let parts = ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        ..Default::default()
    };
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(Arc::new(IpExtractor::new(&[PROXY.to_string()])))
        .with_parts(parts);
    (netray_web::api::router(state.clone()), state)
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON: {e}: {}", self.body))
    }
    fn code(&self) -> Value {
        self.json()["error"]["code"].clone()
    }
}

/// Sends `method uri` as the trusted proxy forwarding for `caller`.
async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    accept: Option<&str>,
    caller: &str,
    body: Option<Value>,
) -> Reply {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-forwarded-for", caller)
        .header("x-request-id", "golden");
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
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

async fn get(app: &Router, uri: &str, accept: Option<&str>) -> Reply {
    send(app, "GET", uri, accept, CALLER, None).await
}

/// Validates `value` against `schema` with the document's components grafted in.
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

/// Validates against the document's `get` operation for `path`.
fn validate_get(path: &str, body: &Value) {
    let doc = serde_json::to_value(openapi()).unwrap();
    let schema = doc["paths"][format!("/api/v1{path}")]["get"]["responses"]["200"]["content"]["application/json"]
        ["schema"]
        .clone();
    validate(schema, body);
}

fn validate_component(name: &str, body: &Value) {
    validate(
        json!({"$ref": format!("#/components/schemas/{name}")}),
        body,
    );
}

/// Parses an SSE body into (event name, data) pairs.
fn frames(body: &str) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for block in body.split("\n\n").filter(|b| !b.trim().is_empty()) {
        let mut name = String::new();
        let mut data = String::new();
        for line in block.lines() {
            if let Some(n) = line.strip_prefix("event:") {
                name = n.trim().to_string();
            } else if let Some(d) = line.strip_prefix("data:") {
                data.push_str(d.trim_start());
            }
        }
        if name.is_empty() && data.is_empty() {
            continue;
        }
        let value: Value =
            serde_json::from_str(&data).unwrap_or_else(|e| panic!("{e}: bad data in {block}"));
        out.push((name, value));
    }
    out
}

fn record_types(v: &Value) -> Vec<String> {
    v["batches"]
        .as_array()
        .unwrap_or_else(|| panic!("no batches: {v}"))
        .iter()
        .map(|b| b["record_type"].as_str().unwrap().to_string())
        .collect()
}

/// Every lookup's `name_server` in a collected query result.
fn name_servers(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for b in v["batches"].as_array().unwrap() {
        for l in b["lookups"]["lookups"].as_array().unwrap() {
            out.push(l["name_server"].as_str().unwrap().to_string());
        }
    }
    out
}

async fn ok_json(app: &Router, uri: &str) -> Value {
    let r = get(app, uri, Some("application/json")).await;
    assert_eq!(r.status, StatusCode::OK, "{uri}: {}", r.body);
    r.json()
}

/// C1, C5: a bare domain runs the default types A, AAAA, CNAME, MX and the result is valid
/// against the document's `get` operation.
#[tokio::test]
async fn c1_c5_a_bare_domain_runs_the_default_types() {
    let (app, _) = setup(ApiConfig::default());
    let v = ok_json(&app, "/dns/query?q=example.com").await;
    assert_eq!(record_types(&v), ["A", "AAAA", "CNAME", "MX"], "{v}");
    validate_get("/dns/query", &v);
    validate_component("DnsQueryResult", &v);
}

/// C4: `MX @google` queries MX only, and only Google servers.
#[tokio::test]
async fn c4_a_named_server_restricts_the_lookups() {
    let (app, _) = setup(ApiConfig::default());
    let v = ok_json(&app, "/dns/query?q=example.com%20MX%20%40google").await;
    assert_eq!(record_types(&v), ["MX"], "{v}");
    let servers = name_servers(&v);
    assert!(!servers.is_empty(), "{v}");
    for s in servers {
        assert!(s.contains("Google"), "{s}");
    }
    validate_component("DnsQueryResult", &v);
}

/// C6: `+dnssec` adds DNSKEY and DS and sets `done.dnssec`.
#[tokio::test]
async fn c6_plus_dnssec_adds_dnskey_and_ds() {
    let (app, _) = setup(ApiConfig::default());
    let v = ok_json(&app, "/dns/query?q=example.com%20A%20%2Bdnssec").await;
    assert_eq!(record_types(&v), ["A", "DNSKEY", "DS"], "{v}");
    assert_eq!(v["done"]["dnssec"], json!(true), "{v}");
}

/// C7: an unrecognized token is a 200 with a warning naming it.
#[tokio::test]
async fn c7_an_unrecognized_token_is_a_warning() {
    let (app, _) = setup(ApiConfig::default());
    let v = ok_json(&app, "/dns/query?q=example.com%20A%20bogus").await;
    let warnings = v["done"]["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("bogus")),
        "{v}"
    );
}

/// C1, C2, C8, C9: a missing or empty q and a parse error are 400 INVALID_INPUT on every GET
/// route; a blocked type names ANY.
#[tokio::test]
async fn c1_c2_c8_c9_bad_q_is_400_invalid_input() {
    let (app, _) = setup(ApiConfig::default());
    let paths = [
        "/dns/query",
        "/dns/compare",
        "/dns/authcompare",
        "/dns/trace",
        "/dns/dnssec",
    ];
    for path in paths {
        for uri in [
            path.to_string(),
            format!("{path}?q="),
            format!("{path}?q=%20"),
            format!("{path}?q=%40google"),
        ] {
            let r = get(&app, &uri, None).await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST, "{uri}: {}", r.body);
            assert_eq!(r.code(), "INVALID_INPUT", "{uri}");
            assert!(r.json()["error"]["message"].is_string(), "{uri}");
        }
    }
    let r = get(&app, "/dns/query?q=example.com%20ANY", None).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.body);
    assert_eq!(r.code(), "INVALID_INPUT");
    assert!(
        r.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("ANY"),
        "{}",
        r.body
    );
}

/// C10: a private server address is refused as the POST route refuses it.
#[tokio::test]
async fn c10_a_private_server_is_refused_as_post_refuses_it() {
    let (app, _) = setup(ApiConfig::default());
    let post = send(
        &app,
        "POST",
        "/dns/query",
        None,
        CALLER,
        Some(json!({"domain": "example.com", "record_types": ["A"], "servers": ["192.168.1.1"]})),
    )
    .await;
    assert_ne!(post.status, StatusCode::OK, "{}", post.body);
    let r = get(&app, "/dns/query?q=example.com%20A%20%40192.168.1.1", None).await;
    assert_eq!(r.status, post.status, "{}", r.body);
    assert_eq!(r.code(), post.code());
}

/// C11: Accept event-stream answers batch, summary and done events.
#[tokio::test]
async fn c11_event_stream_answers_batch_summary_done() {
    let (app, _) = setup(ApiConfig::default());
    let r = get(
        &app,
        "/dns/query?q=example.com%20A",
        Some("text/event-stream"),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let ct = r.headers["content-type"].to_str().unwrap();
    assert!(ct.starts_with("text/event-stream"), "{ct}");
    let events = frames(&r.body);
    let names: Vec<&str> = events.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(names.first(), Some(&"batch"), "{names:?}");
    assert!(names.contains(&"summary"), "{names:?}");
    assert_eq!(names.last(), Some(&"done"), "{names:?}");
}

/// C2, C12, C13: the other four jobs answer their result types over GET.
#[tokio::test]
async fn c2_c12_c13_the_other_jobs_answer_their_result_types() {
    let (app, _) = setup(ApiConfig::default());
    let rows = [
        ("/dns/compare", "example.com%20A", "DnsCompareResult"),
        (
            "/dns/authcompare",
            "example.com%20A",
            "DnsAuthCompareResult",
        ),
        ("/dns/trace", "example.com%20MX", "DnsTraceResult"),
        ("/dns/trace", "example.com", "DnsTraceResult"),
        ("/dns/dnssec", "example.com", "DnsDnssecResult"),
    ];
    for (path, q, component) in rows {
        let v = ok_json(&app, &format!("{path}?q={q}")).await;
        assert!(v["done"].is_object(), "{path}: {v}");
        validate_get(path, &v);
        validate_component(component, &v);
    }
    // C12: compare runs four transports.
    let v = ok_json(&app, "/dns/compare?q=example.com%20A").await;
    let transports: Vec<&str> = v["batches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["transport"].as_str().unwrap())
        .collect();
    assert_eq!(transports, ["udp", "tcp", "tls", "https"], "{v}");
}

/// C3, C14: a routing flag answers 307 with the exact percent-encoded Location, and costs
/// nothing: a caller with a spent budget still gets the redirect.
#[tokio::test]
async fn c3_c14_routing_flags_redirect_307_and_charge_nothing() {
    let (app, state) = setup(api_config("global_burst = 100"));
    let caller: IpAddr = "192.0.2.10".parse().unwrap();
    drop(
        state
            .dns_limiter()
            .check_query_cost(caller, &[], 64, 1)
            .expect("spend the whole per-IP burst"),
    );
    let rows = [
        (
            "example.com%20%2Btrace",
            "/api/v1/dns/trace?q=example.com%20%2Btrace",
        ),
        (
            "example.com%20A%20%2Bcompare",
            "/api/v1/dns/compare?q=example.com%20A%20%2Bcompare",
        ),
        (
            "example.com%20%2Bauth%20%408.8.8.8",
            "/api/v1/dns/authcompare?q=example.com%20%2Bauth%20%408.8.8.8",
        ),
        (
            "example.com%20MX%20%2Bcheck",
            "/api/v1/dns/check?q=example.com%20MX%20%2Bcheck",
        ),
    ];
    for (q, location) in rows {
        let uri = format!("/dns/query?q={q}");
        let r = send(&app, "GET", &uri, None, "192.0.2.10", None).await;
        assert_eq!(
            r.status,
            StatusCode::TEMPORARY_REDIRECT,
            "{uri}: {}",
            r.body
        );
        assert_eq!(r.headers["location"], location, "{uri}");
    }
}

/// C15: a caller over the `[api.dns]` budget gets 429 with Retry-After on GET.
#[tokio::test]
async fn c15_a_caller_over_budget_gets_429_with_retry_after() {
    let (app, state) = setup(api_config("global_burst = 100"));
    let caller: IpAddr = "192.0.2.20".parse().unwrap();
    drop(
        state
            .dns_limiter()
            .check_query_cost(caller, &[], 64, 1)
            .expect("spend the whole per-IP burst"),
    );
    let r = send(
        &app,
        "GET",
        "/dns/query?q=example.com",
        None,
        "192.0.2.20",
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.body);
    assert_eq!(r.code(), "RATE_LIMITED");
    let retry: u64 = r.headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(retry >= 1);
}
