//! API v1 DNS reference routes (spec v2-dns-api, Phase 3): `POST /dns/parse`,
//! `GET /dns/servers`, `GET /dns/record-types` driven through `netray_web::api::router`
//! over the offline DNS stub, paths relative to the mount.
use std::net::SocketAddr;
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
use std::net::IpAddr;
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

fn stub_app() -> Router {
    let parts = ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        ..Default::default()
    };
    router(ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(parts))
}

async fn call(method: &str, path: &str, body: Option<String>) -> (StatusCode, String) {
    let builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("x-request-id", "golden");
    let mut req = builder.body(Body::from(body.unwrap_or_default())).unwrap();
    let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = stub_app().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn parse_raw(body: String) -> (StatusCode, String) {
    call("POST", "/dns/parse", Some(body)).await
}

async fn parse(input: &str) -> Value {
    let (status, body) = parse_raw(json!({"input": input}).to_string()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("not JSON: {e}: {body}"))
}

async fn get(path: &str) -> Value {
    let (status, body) = call("GET", path, None).await;
    assert_eq!(status, StatusCode::OK, "{path}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("not JSON: {e}: {body}"))
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

fn schema_of(path: &str, method: &str) -> Value {
    let doc = serde_json::to_value(openapi()).unwrap();
    doc["paths"][format!("/api/v1{path}")][method]["responses"]["200"]["content"]["application/json"]
        ["schema"]
        .clone()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap_or_else(|| panic!("not an array: {v}"))
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

// C1, C4: partial server token, completions, warning, byte ranges.
#[tokio::test]
async fn parse_partial_server_tokens_completions_and_warning() {
    let v = parse("example.com MX @cl").await;
    let tokens = v["tokens"].as_array().unwrap();
    let got: Vec<(&str, u64, u64)> = tokens
        .iter()
        .map(|t| {
            (
                t["kind"].as_str().unwrap(),
                t["from"].as_u64().unwrap(),
                t["to"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("domain", 0, 11),
            ("record_type", 12, 14),
            ("server_partial", 15, 18)
        ]
    );
    let labels: Vec<&str> = v["completions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"@cloudflare"), "{labels:?}");
    assert_eq!(v["query"]["servers"], json!([]));
    let warnings = strings(&v["query"]["warnings"]);
    assert!(warnings.iter().any(|w| w.contains("@cl")), "{warnings:?}");
}

// C1, C5: a complete query.
#[tokio::test]
async fn parse_complete_query() {
    let v = parse("example.com A @google +tcp").await;
    assert_eq!(v["query"]["domain"], "example.com");
    assert_eq!(v["query"]["record_types"], json!(["A"]));
    assert_eq!(v["query"]["servers"], json!(["google"]));
    assert_eq!(v["query"]["transport"], "tcp");
    assert_eq!(v["error"], Value::Null);
}

// C6: blocked record type.
#[tokio::test]
async fn parse_blocked_type_names_it() {
    let v = parse("example.com ANY").await;
    assert_eq!(v["query"], Value::Null);
    assert!(v["error"].as_str().unwrap().contains("ANY"), "{v}");
}

// C7: empty input.
#[tokio::test]
async fn parse_empty_input() {
    let (status, body) = parse_raw(json!({"input": "", "cursor_pos": 0}).to_string()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["tokens"], json!([]));
    assert_eq!(v["error"], "empty query: no domain specified");
}

// C8, C1: malformed body.
#[tokio::test]
async fn parse_non_json_body_is_400() {
    let (status, body) = parse_raw("not json".to_string()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error"]["code"], "INVALID_INPUT");
}

// C9, C1: @system is not offered under the default config.
#[tokio::test]
async fn parse_does_not_offer_system_by_default() {
    let (status, body) =
        parse_raw(json!({"input": "example.com @", "cursor_pos": 13}).to_string()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    let labels: Vec<&str> = v["completions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"@google"), "{labels:?}");
    assert!(!labels.contains(&"@system"), "{labels:?}");
}

// C2, C10: providers with all four protocols, group aliases.
#[tokio::test]
async fn servers_lists_providers_and_groups() {
    let v = get("/dns/servers").await;
    let providers = v["providers"].as_array().unwrap();
    for name in ["cloudflare", "google", "quad9", "mullvad"] {
        let p = providers
            .iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("provider {name} missing: {v}"));
        let protocols: Vec<String> = p["configs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["protocol"].as_str().unwrap().to_string())
            .collect();
        for proto in ["udp", "tcp", "tls", "https"] {
            assert!(
                protocols.contains(&proto.to_string()),
                "{name}: {protocols:?}"
            );
        }
    }
    let group = |name: &str| -> Vec<String> {
        let mut m = strings(
            &v["groups"]
                .as_array()
                .unwrap()
                .iter()
                .find(|g| g["name"] == name)
                .unwrap_or_else(|| panic!("group {name} missing: {v}"))["providers"],
        );
        m.sort();
        m
    };
    assert_eq!(group("public"), ["cloudflare", "google", "quad9"]);
    assert_eq!(group("all"), ["cloudflare", "google", "mullvad", "quad9"]);
}

// C3, C11: record types.
#[tokio::test]
async fn record_types_exclude_blocked() {
    let v = get("/dns/record-types").await;
    let names: Vec<&str> = v["record_types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for n in ["A", "MX", "DNSKEY", "TLSA", "SVCB"] {
        assert!(names.contains(&n), "{n} missing: {names:?}");
    }
    for n in ["ANY", "AXFR", "IXFR", "OPT", "ZERO", "NULL"] {
        assert!(!names.contains(&n), "{n} present: {names:?}");
    }
}

// C12: each body validates against its documented schema.
#[tokio::test]
async fn reference_bodies_validate_against_documented_schemas() {
    validate(
        schema_of("/dns/parse", "post"),
        &parse("example.com MX @cl").await,
    );
    validate(
        schema_of("/dns/parse", "post"),
        &parse("example.com ANY").await,
    );
    validate(schema_of("/dns/servers", "get"), &get("/dns/servers").await);
    validate(
        schema_of("/dns/record-types", "get"),
        &get("/dns/record-types").await,
    );
}

// C9 (Phase 3 review amendment): with `allow_system_resolvers = true` the v1 route offers `@system`.
#[tokio::test]
async fn parse_offers_system_when_the_config_allows_it() {
    let config: netray_dns::config::ModuleConfig =
        serde_json::from_str(r#"{"dns": {"allow_system_resolvers": true}}"#).unwrap();
    let jobs = netray_dns::testing::stub_jobs_with(config.into_config().unwrap());
    let parts = ApiParts {
        dns: Some(jobs),
        ..Default::default()
    };
    let app = router(ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(parts));
    let body = json!({"input": "example.com @", "cursor_pos": 13}).to_string();
    let mut req = Request::builder()
        .method("POST")
        .uri("/dns/parse")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let offered = v["completions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["label"] == "@system");
    assert!(offered, "{v}");
}

// Final review (R10-R12): the catalogue, the type list and parse answer without a DNS part; parse
// then offers no `@system`.
#[tokio::test]
async fn reference_routes_answer_without_a_dns_part() {
    let app = router(ApiState::new(ApiConfig::default(), Arc::new(NoRuns)));
    for (method, path, body) in [
        ("GET", "/dns/servers", None),
        ("GET", "/dns/record-types", None),
        (
            "POST",
            "/dns/parse",
            Some(json!({"input": "example.com @", "cursor_pos": 13}).to_string()),
        ),
    ] {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.unwrap_or_default()))
            .unwrap();
        let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(peer));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{method} {path}");
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        if path == "/dns/parse" {
            assert!(
                v["completions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|c| c["label"] != "@system"),
                "{v}"
            );
        }
    }
}
