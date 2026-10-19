//! API v1 DNS result cache and replay (spec v2-dns-api, Phase 4, requirements 13, 14): every v1
//! DNS job stores its collected result at `done` under `done.cache_key`; `GET /dns/results/{key}`
//! replays it. Driven through `netray_web::api::router` over the offline DNS stub, paths relative
//! to the mount. One app instance serves the job and the replay: the cache lives in the `ApiState`.
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

fn stub_app() -> Router {
    let config: ApiConfig =
        toml::from_str("[dns]\nper_ip_burst = 200\nglobal_burst = 200").expect("[api.dns] parses");
    let parts = ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        ..Default::default()
    };
    router(ApiState::new(config, Arc::new(NoRuns)).with_parts(parts))
}

struct Reply {
    status: StatusCode,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON: {e}: {}", self.body))
    }
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    accept: Option<&str>,
    body: Option<Value>,
) -> Reply {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
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
    let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
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

fn is_key(s: &str) -> bool {
    s.len() == 12 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
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

/// The `done.cache_key` of a JSON job body.
fn json_key(body: &Value) -> String {
    let key = body["done"]["cache_key"]
        .as_str()
        .unwrap_or_else(|| panic!("done has no cache_key: {body}"));
    assert!(is_key(key), "not 12 lowercase hex: {key}");
    key.to_string()
}

async fn replay(app: &Router, key: &str) -> Value {
    let r = send(app, "GET", &format!("/dns/results/{key}"), None, None).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    r.json()
}

/// C1, C3: POST /dns/query as JSON carries a 12-hex key; the replay answers mode query, query
/// example.com and a result equal to the original body.
#[tokio::test]
async fn query_json_key_replays_the_original_body() {
    let app = stub_app();
    let body = json!({"domain": "example.com", "record_types": ["A"]});
    let r = send(
        &app,
        "POST",
        "/dns/query",
        Some("application/json"),
        Some(body),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let original = r.json();
    let key = json_key(&original);

    let replayed = replay(&app, &key).await;
    assert_eq!(replayed["key"], json!(key));
    assert_eq!(replayed["mode"], json!("query"));
    assert_eq!(replayed["query"], json!("example.com"));
    assert_eq!(replayed["result"], original);
}

/// C1, C4: GET /dns/query?q=example.com MX streamed; the done frame's key replays with the q and
/// the summary included.
#[tokio::test]
async fn query_get_stream_key_replays_with_summary() {
    let app = stub_app();
    let r = send(
        &app,
        "GET",
        "/dns/query?q=example.com%20MX",
        Some("text/event-stream"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let events = frames(&r.body);
    let done = events
        .iter()
        .find(|f| f.0 == "done")
        .unwrap_or_else(|| panic!("no done frame: {events:?}"));
    let key = done.1["cache_key"]
        .as_str()
        .unwrap_or_else(|| panic!("done has no cache_key: {}", done.1));
    assert!(is_key(key), "{key}");

    let replayed = replay(&app, key).await;
    assert_eq!(replayed["mode"], json!("query"));
    assert_eq!(replayed["query"], json!("example.com MX"));
    assert!(replayed["result"]["summary"].is_object(), "{replayed}");
    assert!(
        !replayed["result"]["batches"].as_array().unwrap().is_empty(),
        "{replayed}"
    );
}

/// C1, C5: compare, authcompare, trace and dnssec, in POST and GET forms, each carry a key that
/// replays with its mode.
#[tokio::test]
async fn other_jobs_keys_replay_with_their_mode() {
    let app = stub_app();
    for mode in ["compare", "authcompare", "trace", "dnssec"] {
        let path = format!("/dns/{mode}");
        let r = send(
            &app,
            "POST",
            &path,
            Some("application/json"),
            Some(json!({"domain": "example.com"})),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{path}: {}", r.body);
        let replayed = replay(&app, &json_key(&r.json())).await;
        assert_eq!(replayed["mode"], json!(mode), "POST {path}");
        assert_eq!(replayed["query"], json!("example.com"), "POST {path}");

        let r = send(
            &app,
            "GET",
            &format!("{path}?q=example.com"),
            Some("application/json"),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "GET {path}: {}", r.body);
        let replayed = replay(&app, &json_key(&r.json())).await;
        assert_eq!(replayed["mode"], json!(mode), "GET {path}");
    }
}

/// C6: two runs of the same job get different keys.
#[tokio::test]
async fn two_runs_get_different_keys() {
    let app = stub_app();
    let body = json!({"domain": "example.com", "record_types": ["A"]});
    let a = send(&app, "POST", "/dns/query", None, Some(body.clone())).await;
    let b = send(&app, "POST", "/dns/query", None, Some(body)).await;
    assert_ne!(json_key(&a.json()), json_key(&b.json()));
}

/// C2, C7: a malformed key answers 400 INVALID_INPUT, an unknown 12-hex key 404 NOT_FOUND.
#[tokio::test]
async fn replay_refuses_malformed_and_unknown_keys() {
    let app = stub_app();
    for key in ["XYZ", "0123456789abc", "0123456789AB"] {
        let r = send(&app, "GET", &format!("/dns/results/{key}"), None, None).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{key}: {}", r.body);
        assert_eq!(r.json()["error"]["code"], json!("INVALID_INPUT"), "{key}");
    }
    let r = send(&app, "GET", "/dns/results/0123456789ab", None, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.body);
    assert_eq!(r.json()["error"]["code"], json!("NOT_FOUND"));
}

/// C8: prism's V1 dnssec route streamed ends in a done without a cache_key.
#[tokio::test]
async fn prism_dnssec_done_has_no_cache_key() {
    let prism = netray_dns::api::api_router(netray_dns::testing::stub_state())
        .layer(axum::middleware::from_fn(netray_dns::request_id_middleware));
    let mut req = Request::builder()
        .method("POST")
        .uri("/api/dnssec")
        .header("content-type", "application/json")
        .header("x-request-id", "golden")
        .body(Body::from(json!({"domain": "example.com"}).to_string()))
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = prism.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let events = frames(std::str::from_utf8(&bytes).unwrap());
    let done = events
        .iter()
        .find(|f| f.0 == "done")
        .unwrap_or_else(|| panic!("no done frame: {events:?}"));
    assert!(
        done.1.as_object().unwrap().get("cache_key").is_none(),
        "{}",
        done.1
    );
}

/// C9: a replay validates against DnsCachedResult and the operation's documented 200 schema.
#[tokio::test]
async fn replay_validates_against_its_documented_schema() {
    let app = stub_app();
    let r = send(
        &app,
        "POST",
        "/dns/query",
        None,
        Some(json!({"domain": "example.com", "record_types": ["A"]})),
    )
    .await;
    let replayed = replay(&app, &json_key(&r.json())).await;
    validate(
        json!({"$ref": "#/components/schemas/DnsCachedResult"}),
        &replayed,
    );
    let doc = serde_json::to_value(openapi()).unwrap();
    let schema = doc["paths"]["/api/v1/dns/results/{key}"]["get"]["responses"]["200"]["content"]
        ["application/json"]["schema"]
        .clone();
    validate(schema, &replayed);
}
