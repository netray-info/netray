//! API v1 DNS inspector jobs (spec v2-api, Phase 7): `POST /dns/*` driven through
//! `netray_web::api::router` over the offline DNS stub, paths relative to the mount.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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

fn stub_app() -> Router {
    let parts = ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        ..Default::default()
    };
    router(ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(parts))
}

struct Reply {
    status: StatusCode,
    content_type: String,
    body: String,
}

async fn post(path: &str, accept: Option<&str>, body: Value) -> Reply {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("x-request-id", "golden");
    if let Some(a) = accept {
        builder = builder.header("accept", a);
    }
    let mut req = builder.body(Body::from(body.to_string())).unwrap();
    let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = stub_app().oneshot(req).await.unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        content_type,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

fn json_of(r: &Reply) -> Value {
    serde_json::from_str(&r.body).unwrap_or_else(|e| panic!("body is not JSON: {e}: {}", r.body))
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

fn validate_result(path: &str, body: &Value) {
    let doc = serde_json::to_value(openapi()).unwrap();
    let schema = doc["paths"][format!("/api/v1{path}")]["post"]["responses"]["200"]["content"]["application/json"]
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

/// Parses an SSE body into (event name, data) pairs; comment lines are skipped.
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

fn normalise(v: &mut Value) {
    match v {
        Value::Object(map) => {
            if map.contains_key("duration_ms") {
                map.insert("duration_ms".into(), json!(0));
            }
            for child in map.values_mut() {
                normalise(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalise),
        _ => {}
    }
}

fn normalised(frames: Vec<(String, Value)>, drop_cache_key: bool) -> Vec<(String, Value)> {
    frames
        .into_iter()
        .map(|(name, mut data)| {
            if drop_cache_key && let Some(map) = data.as_object_mut() {
                map.remove("cache_key");
            }
            normalise(&mut data);
            (name, data)
        })
        .collect()
}

/// C3: Accept application/json answers one typed collected object valid against `DnsQueryResult`.
#[tokio::test]
async fn query_json_is_a_valid_collected_object() {
    let body = json!({"domain": "example.com", "record_types": ["A"]});
    let r = post("/dns/query", Some("application/json"), body).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("application/json"),
        "{}",
        r.content_type
    );
    let value = json_of(&r);
    assert_eq!(value["batches"].as_array().unwrap().len(), 1, "{value}");
    assert!(value["done"].is_object(), "{value}");
    validate_result("/dns/query", &value);
    validate_component("DnsQueryResult", &value);
}

/// C3: no Accept header answers JSON too.
#[tokio::test]
async fn query_without_accept_answers_json() {
    let body = json!({"domain": "example.com", "record_types": ["A"]});
    let r = post("/dns/query", None, body).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("application/json"),
        "{}",
        r.content_type
    );
    validate_component("DnsQueryResult", &json_of(&r));
}

/// C4: Accept event-stream answers named events, each data valid against its component, and
/// v1's frames equal prism's `/api/query` frames over the same stub.
#[tokio::test]
async fn query_event_stream_names_components_and_equals_prism() {
    let body = json!({"domain": "example.com", "record_types": ["A"]});
    let r = post("/dns/query", Some("text/event-stream"), body.clone()).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("text/event-stream"),
        "{}",
        r.content_type
    );
    let v1 = frames(&r.body);
    assert_eq!(v1.first().map(|f| f.0.as_str()), Some("batch"), "{v1:?}");
    assert_eq!(v1.last().map(|f| f.0.as_str()), Some("done"), "{v1:?}");
    for (name, data) in &v1 {
        let component = match name.as_str() {
            "batch" => "DnsBatch",
            "error" => "DnsErrorEvent",
            "enrichment" => "DnsEnrichmentEvent",
            "done" => "DnsQueryDone",
            "summary" => "DnsJobSummary",
            other => panic!("unexpected event {other}"),
        };
        validate_component(component, data);
    }

    let prism = netray_dns::api::api_router(netray_dns::testing::stub_state())
        .layer(axum::middleware::from_fn(netray_dns::request_id_middleware));
    let mut req = Request::builder()
        .method("POST")
        .uri("/api/query")
        .header("content-type", "application/json")
        .header("x-request-id", "golden")
        .body(Body::from(body.to_string()))
        .unwrap();
    let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345);
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = prism.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let prism_frames = frames(std::str::from_utf8(&bytes).unwrap());
    // C20 (v2-dns-api): prism emits no `summary`; v1's frames without it equal prism's.
    assert!(
        prism_frames.iter().all(|f| f.0 != "summary"),
        "prism emitted a summary event"
    );
    let v1: Vec<_> = v1.into_iter().filter(|f| f.0 != "summary").collect();
    assert_eq!(
        normalised(v1, false),
        normalised(prism_frames, true),
        "v1 frames differ from prism's"
    );
}

/// C5: trace, compare, authcompare and dnssec answer 200 in both forms.
#[tokio::test]
async fn other_jobs_answer_in_both_forms() {
    let rows: [(&str, Value, &[&str], &str); 4] = [
        (
            "/dns/trace",
            json!({"domain": "example.com"}),
            &["hop", "error", "enrichment", "done"],
            "DnsTraceDone",
        ),
        (
            "/dns/compare",
            json!({"domain": "example.com", "record_types": ["A"]}),
            &["batch", "error", "enrichment", "summary", "done"],
            "DnsCompareDone",
        ),
        (
            "/dns/authcompare",
            json!({"domain": "example.com", "record_types": ["A"]}),
            &["batch", "error", "enrichment", "summary", "done"],
            "DnsAuthCompareDone",
        ),
        (
            "/dns/dnssec",
            json!({"domain": "example.com"}),
            &["chain", "error", "done"],
            "DnsDnssecDone",
        ),
    ];
    for (path, body, names, done) in rows {
        let r = post(path, Some("application/json"), body.clone()).await;
        assert_eq!(r.status, StatusCode::OK, "{path}: {}", r.body);
        let value = json_of(&r);
        assert!(value["done"].is_object(), "{path}: {value}");
        validate_result(path, &value);

        let r = post(path, Some("text/event-stream"), body).await;
        assert_eq!(r.status, StatusCode::OK, "{path}: {}", r.body);
        assert!(
            r.content_type.starts_with("text/event-stream"),
            "{path}: {}",
            r.content_type
        );
        let events = frames(&r.body);
        assert_eq!(
            events.last().map(|f| f.0.as_str()),
            Some("done"),
            "{path}: {events:?}"
        );
        for (name, data) in &events {
            assert!(names.contains(&name.as_str()), "{path}: event {name}");
            let component = match name.as_str() {
                "batch" => "DnsBatch",
                "hop" => "DnsHopEvent",
                "chain" => "DnsChainEvent",
                "error" => "DnsErrorEvent",
                "enrichment" => "DnsEnrichmentEvent",
                "summary" => "DnsJobSummary",
                _ => done,
            };
            validate_component(component, data);
        }
    }
}
