// prism's five job routes answer unchanged over the offline stubs (specs/features/v2-api Phase 7).
//
// Each route is driven through `api_router` over `netray_dns::testing::stub_state()`, with the
// request header `x-request-id: golden`, once as SSE and once with `?stream=false`. The answers
// are compared with the goldens `tests/fixtures/contracts/prism-{query,compare,authcompare,trace,
// dnssec}.{sse,json}` after `duration_ms`/`latency_ms` -> 0 and `cache_key` -> "KEY"; batch frames are sorted
// by (transport|source, record_type) with `completed` zeroed, as their arrival order is not fixed.
//
// UPDATE_GOLDEN=1 cargo test -p netray-dns --features testing --test jobs_prism   (re)writes them.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_dns::api::api_router;
use netray_dns::testing::stub_state;
use serde_json::{Value, json};
use tower::ServiceExt;

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name)
}

fn assert_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "golden {} is missing; run with UPDATE_GOLDEN=1 to write it",
            path.display()
        )
    });
    assert!(
        committed == actual,
        "golden {} differs from what prism answers now; if the change is intended, run with UPDATE_GOLDEN=1 and commit the result",
        path.display()
    );
}

fn router() -> axum::Router {
    api_router(stub_state()).layer(axum::middleware::from_fn(netray_dns::request_id_middleware))
}

fn peer() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 77)), 12345)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("x-request-id", "golden")
        .extension(ConnectInfo::<SocketAddr>(peer()))
        .body(Body::empty())
        .unwrap()
}

fn post(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-request-id", "golden")
        .extension(ConnectInfo::<SocketAddr>(peer()))
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn answer(router: &axum::Router, req: Request<Body>) -> (StatusCode, String) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

/// `duration_ms` and `latency_ms` -> 0 and `cache_key` -> "KEY" (a string one), recursively;
/// `lookups` and `auth_servers` arrays sorted, as authcompare's server list comes from a hash map
/// and its order is not fixed.
fn normalise(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                match k.as_str() {
                    "duration_ms" | "latency_ms" => *val = json!(0),
                    "cache_key" if val.is_string() => *val = json!("KEY"),
                    "lookups" | "auth_servers" => {
                        normalise(val);
                        if let Value::Array(items) = val {
                            items.sort_by_key(|item| item.to_string());
                        }
                    }
                    _ => normalise(val),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalise),
        _ => {}
    }
}

/// Events as `(type, data)`, normalised; the batch frames are sorted among themselves.
fn normalise_events(mut events: Vec<(String, Value)>) -> Vec<(String, Value)> {
    for (_, data) in events.iter_mut() {
        normalise(data);
    }
    let key = |data: &Value| {
        let group = data
            .get("transport")
            .or_else(|| data.get("source"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let record_type = data
            .get("record_type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        (group, record_type)
    };
    let slots: Vec<usize> = (0..events.len())
        .filter(|i| events[*i].0 == "batch")
        .collect();
    let mut batches: Vec<(String, Value)> = slots.iter().map(|i| events[*i].clone()).collect();
    for (_, data) in batches.iter_mut() {
        if let Some(c) = data.get_mut("completed") {
            *c = json!(0);
        }
    }
    batches.sort_by_key(|(_, data)| key(data));
    for (slot, batch) in slots.into_iter().zip(batches) {
        events[slot] = batch;
    }
    events
}

fn sse_frames(body: &str) -> Vec<(String, Value)> {
    body.split("\n\n")
        .filter_map(|frame| {
            let mut name = None;
            let mut data = None;
            for line in frame.lines() {
                if let Some(v) = line.strip_prefix("event:") {
                    name = Some(v.trim().to_string());
                } else if let Some(v) = line.strip_prefix("data:") {
                    data = Some(serde_json::from_str::<Value>(v.trim()).unwrap());
                }
            }
            Some((name?, data?))
        })
        .collect()
}

fn render_sse(events: &[(String, Value)]) -> String {
    events
        .iter()
        .map(|(name, data)| format!("event: {name}\ndata: {data}\n\n"))
        .collect()
}

fn render_json(events: Vec<(String, Value)>, truncated: bool) -> String {
    let events: Vec<Value> = events
        .into_iter()
        .map(|(t, d)| json!({ "type": t, "data": d }))
        .collect();
    let mut s =
        serde_json::to_string_pretty(&json!({ "events": events, "truncated": truncated })).unwrap();
    s.push('\n');
    s
}

struct Case {
    stem: &'static str,
    request: fn(&str) -> Request<Body>,
}

fn post_query(suffix: &str) -> Request<Body> {
    post(
        &format!("/api/query{suffix}"),
        &json!({ "domain": "example.com", "record_types": ["A"] }),
    )
}

fn get_query(suffix: &str) -> Request<Body> {
    let sep = if suffix.is_empty() { "" } else { "&" };
    get(&format!(
        "/api/query?q=example.com%20A{sep}{}",
        suffix.trim_start_matches('?')
    ))
}

fn post_compare(suffix: &str) -> Request<Body> {
    post(
        &format!("/api/compare{suffix}"),
        &json!({ "domain": "example.com", "record_types": ["A", "MX"] }),
    )
}

fn post_authcompare(suffix: &str) -> Request<Body> {
    post(
        &format!("/api/authcompare{suffix}"),
        &json!({ "domain": "example.com", "record_types": ["A", "MX"] }),
    )
}

fn post_trace(suffix: &str) -> Request<Body> {
    post(
        &format!("/api/trace{suffix}"),
        &json!({ "domain": "example.com" }),
    )
}

fn post_dnssec(suffix: &str) -> Request<Body> {
    post(
        &format!("/api/dnssec{suffix}"),
        &json!({ "domain": "example.com" }),
    )
}

#[tokio::test]
async fn prism_job_routes_answer_their_goldens() {
    let cases = [
        Case {
            stem: "prism-query",
            request: post_query,
        },
        Case {
            stem: "prism-query",
            request: get_query,
        },
        Case {
            stem: "prism-compare",
            request: post_compare,
        },
        Case {
            stem: "prism-authcompare",
            request: post_authcompare,
        },
        Case {
            stem: "prism-trace",
            request: post_trace,
        },
        Case {
            stem: "prism-dnssec",
            request: post_dnssec,
        },
    ];
    for case in &cases {
        let router = router();

        let (status, body) = answer(&router, (case.request)("")).await;
        assert_eq!(status, StatusCode::OK, "{} sse: {body}", case.stem);
        let sse = render_sse(&normalise_events(sse_frames(&body)));
        assert!(
            sse.contains("event: done"),
            "{} sse: no done\n{sse}",
            case.stem
        );
        assert_golden(&format!("{}.sse", case.stem), &sse);

        let (status, body) = answer(&router, (case.request)("?stream=false")).await;
        assert_eq!(status, StatusCode::OK, "{} json: {body}", case.stem);
        let v: Value = serde_json::from_str(&body).unwrap();
        let events: Vec<(String, Value)> = v["events"]
            .as_array()
            .unwrap_or_else(|| panic!("{} json: no events array: {body}", case.stem))
            .iter()
            .map(|e| (e["type"].as_str().unwrap().to_string(), e["data"].clone()))
            .collect();
        let truncated = v["truncated"].as_bool().unwrap();
        assert!(!truncated, "{} json: truncated", case.stem);
        let json = render_json(normalise_events(events), truncated);
        assert_golden(&format!("{}.json", case.stem), &json);
    }
}

#[tokio::test]
async fn results_of_a_streamed_query_answer_its_cached_events() {
    let router = router();
    let (status, body) = answer(&router, post_query("")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let frames = sse_frames(&body);
    let (done_name, done) = frames.last().expect("the stream carries events");
    assert_eq!(done_name, "done", "{body}");
    let key = done["cache_key"].as_str().expect("done carries cache_key");

    let (status, cached) = answer(&router, get(&format!("/api/results/{key}"))).await;
    assert_eq!(status, StatusCode::OK, "{cached}");
    let cached: Value = serde_json::from_str(&cached).unwrap();
    assert_eq!(cached["mode"], "query");
    assert_eq!(cached["query"], "example.com");
    let names: Vec<&str> = cached["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_type"].as_str().unwrap())
        .collect();
    let streamed: Vec<&str> = frames.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, streamed);
    let last = cached["events"].as_array().unwrap().last().unwrap();
    assert_eq!(last["data"]["cache_key"], key);
}

#[tokio::test]
async fn prism_openapi_names_no_dns_component() {
    let router = router();
    let (status, body) = answer(&router, get("/api-docs/openapi.json")).await;
    assert_eq!(status, StatusCode::OK);
    let doc: Value = serde_json::from_str(&body).unwrap();
    let names: Vec<&String> = doc["components"]["schemas"]
        .as_object()
        .unwrap()
        .keys()
        // `DnssecRequest` is prism's own request body, older than v1's `Dns*` components.
        .filter(|k| k.starts_with("Dns") && k.as_str() != "DnssecRequest")
        .collect();
    assert!(names.is_empty(), "prism's document names {names:?}");
}

// spec v2-input-limits, Phase 2 (R4): prism's V1 route refuses ten types plus `+dnssec` with
// the record-type limit's refusal.
#[tokio::test]
async fn input_limits_prism_query_ten_types_plus_dnssec_is_too_many_record_types() {
    let req = Request::builder()
        .uri("/api/query?q=example.com%20A%20AAAA%20MX%20TXT%20NS%20SOA%20CAA%20CNAME%20PTR%20SRV%20%2Bdnssec")
        .extension(ConnectInfo::<SocketAddr>(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            12345,
        )))
        .body(Body::empty())
        .unwrap();
    let resp = router().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).expect("JSON error body");
    assert_eq!(body["error"]["code"], "TOO_MANY_RECORD_TYPES", "{body}");
}
