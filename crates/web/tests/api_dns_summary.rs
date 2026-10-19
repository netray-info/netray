//! API v1 DNS comparison summary (spec v2-dns-api, Phase 1): `POST /dns/*` through
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

fn q(domain: &str, types: &[&str]) -> Value {
    json!({"domain": domain, "record_types": types})
}

async fn json_job(path: &str, body: Value) -> Value {
    let r = post(path, Some("application/json"), body).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    json_of(&r)
}

fn answers(v: &Value, i: usize) -> &Vec<Value> {
    v["summary"]["types"][i]["answers"]
        .as_array()
        .unwrap_or_else(|| panic!("no summary: {v}"))
}

/// C7: a two-server agreeing query summarises into one agreeing type.
#[tokio::test]
async fn c7_query_summary_agrees() {
    let v = json_job("/dns/query", q("example.com", &["A"])).await;
    let types = v["summary"]["types"]
        .as_array()
        .unwrap_or_else(|| panic!("no summary: {v}"));
    assert_eq!(types.len(), 1, "{v}");
    assert_eq!(types[0]["record_type"], "A");
    assert_eq!(types[0]["agreement"], "agree");
    let a = answers(&v, 0);
    assert_eq!(a.len(), 2, "{v}");
    let names: Vec<&str> = a.iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert!(
        names.contains(&"Google 1") && names.contains(&"Google 2"),
        "{names:?}"
    );
    for ans in a {
        assert_eq!(ans["outcome"], "answered");
        assert_eq!(ans["records"][0]["data"], "192.0.2.10");
        assert_eq!(ans["transport"], "udp");
        assert!(ans["response_time_ms"].is_number(), "{ans}");
    }
    assert_eq!(v["summary"]["agree"], 1);
    validate_component("DnsQueryResult", &v);
}

/// C8: servers answering differently diverge.
#[tokio::test]
async fn c8_query_summary_diverges() {
    let v = json_job("/dns/query", q("diverge.example.com", &["A"])).await;
    assert_eq!(v["summary"]["types"][0]["agreement"], "diverge", "{v}");
    for ans in answers(&v, 0) {
        let addr = ans["address"].as_str().unwrap();
        let want = if addr.starts_with("8.8.8.8") {
            "192.0.2.10"
        } else if addr.starts_with("8.8.4.4") {
            "192.0.2.20"
        } else {
            panic!("unexpected server {addr}")
        };
        assert_eq!(ans["records"][0]["data"], want, "{ans}");
    }
}

/// C9: SERVFAIL everywhere is unmeasured, never a verdict.
#[tokio::test]
async fn c9_servfail_is_unmeasured() {
    let v = json_job("/dns/query", q("dnssec-failed.org", &["A"])).await;
    assert_eq!(v["summary"]["types"][0]["agreement"], "unmeasured", "{v}");
    let a = answers(&v, 0);
    assert!(!a.is_empty(), "{v}");
    for ans in a {
        assert_eq!(ans["outcome"], "unmeasured", "{ans}");
        assert_eq!(ans["response_code"], "SERVFAIL", "{ans}");
    }
}

/// C10: one failed type does not taint the other.
#[tokio::test]
async fn c10_partial_failure_counts() {
    let v = json_job("/dns/query", q("partial.example.net", &["A", "AAAA"])).await;
    let types = v["summary"]["types"]
        .as_array()
        .unwrap_or_else(|| panic!("no summary: {v}"));
    let of = |t: &str| {
        types
            .iter()
            .find(|x| x["record_type"] == t)
            .unwrap_or_else(|| panic!("{t}: {v}"))
    };
    assert_eq!(of("A")["agreement"], "agree");
    assert_eq!(of("AAAA")["agreement"], "unmeasured");
    assert_eq!(v["summary"]["agree"], 1);
    assert_eq!(v["summary"]["unmeasured"], 1);
    assert_eq!(v["summary"]["record_types"], 2);
}

/// C11 (API form): NODATA is a measured no_records answer.
#[tokio::test]
async fn c11_nodata_is_no_records() {
    let v = json_job("/dns/query", q("example.com", &["AAAA"])).await;
    assert_eq!(v["summary"]["types"][0]["agreement"], "agree", "{v}");
    let a = answers(&v, 0);
    assert!(!a.is_empty(), "{v}");
    for ans in a {
        assert_eq!(ans["outcome"], "no_records", "{ans}");
    }
}

/// C16: an SOA record carries its serial age in days.
#[tokio::test]
async fn c16_soa_serial_age() {
    let v = json_job("/dns/query", q("soa.example.com", &["SOA"])).await;
    let rec = &answers(&v, 0)[0]["records"][0];
    assert!(
        rec["data"].as_str().unwrap().contains("2026101501"),
        "{rec}"
    );
    // 2026-10-15 is day 20741 since 1970-01-01.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let expected = (secs / 86400) as i64 - 20741;
    assert_eq!(rec["soa_serial_age_days"], json!(expected), "{rec}");
}

/// C17: compare queries four transports and they agree.
#[tokio::test]
async fn c17_compare_summary_has_four_transports() {
    let v = json_job("/dns/compare", q("example.com", &["A"])).await;
    let mut transports: Vec<&str> = answers(&v, 0)
        .iter()
        .map(|x| x["transport"].as_str().unwrap())
        .collect();
    transports.sort();
    transports.dedup();
    assert_eq!(transports, ["https", "tcp", "tls", "udp"], "{v}");
    assert_eq!(v["summary"]["types"][0]["agreement"], "agree", "{v}");
    validate_component("DnsCompareResult", &v);
}

/// C18: authcompare answers carry their source.
#[tokio::test]
async fn c18_authcompare_summary_has_sources() {
    let v = json_job("/dns/authcompare", q("example.com", &["A"])).await;
    let sources: Vec<&str> = answers(&v, 0)
        .iter()
        .filter_map(|x| x["source"].as_str())
        .collect();
    assert!(sources.contains(&"recursive"), "{v}");
    assert!(sources.contains(&"authoritative"), "{v}");
    assert_eq!(v["summary"]["types"][0]["agreement"], "agree", "{v}");
}

fn zero_times(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for key in ["response_time_ms", "median_response_time_ms"] {
                if map.contains_key(key) {
                    map.insert(key.into(), json!(0));
                }
            }
            map.values_mut().for_each(zero_times);
        }
        Value::Array(items) => items.iter_mut().for_each(zero_times),
        _ => {}
    }
}

/// C19: each stream carries one `summary` frame right before `done`; the query one equals
/// the JSON form's summary.
#[tokio::test]
async fn c19_streams_emit_summary_before_done() {
    for path in ["/dns/query", "/dns/compare", "/dns/authcompare"] {
        let r = post(path, Some("text/event-stream"), q("example.com", &["A"])).await;
        assert_eq!(r.status, StatusCode::OK, "{path}: {}", r.body);
        let events = frames(&r.body);
        let n = events.len();
        assert!(n >= 2, "{path}: {events:?}");
        assert_eq!(events[n - 1].0, "done", "{path}: {events:?}");
        assert_eq!(events[n - 2].0, "summary", "{path}: {events:?}");
        assert_eq!(
            events.iter().filter(|f| f.0 == "summary").count(),
            1,
            "{path}"
        );
        if path == "/dns/query" {
            let mut streamed = events[n - 2].1.clone();
            validate_component("DnsJobSummary", &streamed);
            let mut json_form = json_job(path, q("example.com", &["A"])).await["summary"].clone();
            zero_times(&mut streamed);
            zero_times(&mut json_form);
            assert_eq!(streamed, json_form);
        }
    }
}

/// C5: the collected result always serialises `summary` (null until done).
#[test]
fn c5_summary_is_required_in_collected_schema() {
    let doc = serde_json::to_value(openapi()).unwrap();
    let required = doc["components"]["schemas"]["DnsQueryResult"]["required"]
        .as_array()
        .expect("required list");
    assert!(required.iter().any(|r| r == "summary"), "{required:?}");
}

/// C21: trace answer records and dnssec levels already carry their documented fields.
/// The stub trace sets no `server_name` on the answering server, so only `answer_records`
/// is asserted.
#[tokio::test]
async fn c21_trace_and_dnssec_fields() {
    let v = json_job("/dns/trace", json!({"domain": "example.com"})).await;
    let found = v["hops"].as_array().unwrap().iter().any(|h| {
        h["hop"]["server_results"].as_array().is_some_and(|rs| {
            rs.iter().any(|s| {
                s["answer_records"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|r| r["rdata"] == "192.0.2.10"))
            })
        })
    });
    assert!(found, "{v}");

    let v = json_job("/dns/dnssec", json!({"domain": "example.com"})).await;
    let levels = v["levels"].as_array().unwrap();
    assert!(!levels.is_empty(), "{v}");
    for l in levels {
        for key in ["dnskey_records", "ds_records", "rrsig_records"] {
            assert!(l["level"][key].is_array(), "{key}: {l}");
        }
    }
}
