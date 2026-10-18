//! v2-api requirement 6: every case's status is documented in the committed document and its
//! body validates against the documented schema; requirement 6's coverage clause (C23).
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request};
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_http::testing::StubHttpInspect;
use netray_ip::testing::StubIpLookup;
use netray_model::CheckResult;
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_tls::testing::{StubTlsBackend, stub_state};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RefusalReason, RunEvent, RunRefusal,
    RunStart, RunTrigger, router,
};
use serde_json::Value;
use tower::ServiceExt;

/// Scripted by domain: `ok.example.com` is admitted with a fixed event list ending in
/// `Recorded(Some(First))`, `throttled.example.com` is refused by the throttle,
/// `pre.example.com` is not started (prefetch); any other domain is not started.
struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        if domain == "bad_domain" {
            return Err(DomainRefusal {
                code: "DOMAIN_INVALID",
                message: "not a domain".into(),
            });
        }
        Ok(domain.to_lowercase())
    }
    fn start<'a>(
        &'a self,
        domain: &'a str,
        _client: std::net::IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = RunStart> + Send + 'a>> {
        Box::pin(async move {
            match domain {
                "ok.example.com" => {
                    let events = vec![
                        RunEvent::Section(SectionEvent {
                            protocol: Protocol::Dns,
                            outcome: SectionOutcome::Measured {
                                checks: vec![CheckResult {
                                    id: CheckId::parse("dns.dnssec_valid").unwrap(),
                                    status: Status::Pass,
                                    findings: vec![],
                                    evidence: vec![BlockId::parse("dns.caa").unwrap()],
                                }],
                                evidence: vec![Block {
                                    id: BlockId::parse("dns.caa").unwrap(),
                                    title: "CAA".into(),
                                    shape: Shape::Text("0 issue \"example.org\"".into()),
                                }],
                            },
                        }),
                        RunEvent::Done(run(Status::Pass, vec![])),
                        RunEvent::Recorded(Some(Recorded::First {
                            id: "AbCd1234".into(),
                        })),
                    ];
                    RunStart::Admitted(Box::pin(futures::stream::iter(events)))
                }
                "throttled.example.com" => RunStart::Refused(RunRefusal {
                    reason: RefusalReason::Throttle,
                    retry_after_s: 42,
                    last_run_ago_s: Some(5),
                    last_incomplete: false,
                }),
                _ => RunStart::NotStarted,
            }
        })
    }
}

/// One validated request: `doc_path` is the document key (full path template), `uri` the request
/// path relative to the mount, `warmup` the requests sent first from the same client.
#[derive(Clone, Copy, PartialEq)]
enum Store {
    /// No history store: the domain and entry endpoints answer 503.
    Absent,
    /// A temp store seeded with `seed()`; `{e1}`, `{e2}` and `{f1}` in the uri are the entry IDs.
    Seeded,
}

struct Case {
    store: Store,
    doc_path: &'static str,
    method: &'static str,
    uri: &'static str,
    status: u16,
    warmup: usize,
    /// The `Accept` header sent, if any.
    accept: Option<&'static str>,
    /// A JSON request body, if any.
    body: Option<&'static str>,
    /// No IP module in the parts: every IP endpoint answers 503.
    ip_off: bool,
}

const fn case(
    doc_path: &'static str,
    method: &'static str,
    uri: &'static str,
    status: u16,
    warmup: usize,
) -> Case {
    Case {
        store: Store::Absent,
        doc_path,
        method,
        uri,
        status,
        warmup,
        accept: None,
        body: None,
        ip_off: false,
    }
}

const fn seeded(doc_path: &'static str, uri: &'static str, status: u16) -> Case {
    Case {
        store: Store::Seeded,
        doc_path,
        method: "get",
        uri,
        status,
        warmup: 0,
        accept: None,
        body: None,
        ip_off: false,
    }
}

const RUNS: &str = "/api/v1/domains/{domain}/runs";

const fn post_run(uri: &'static str, status: u16, accept: Option<&'static str>) -> Case {
    Case {
        store: Store::Absent,
        doc_path: RUNS,
        method: "post",
        uri,
        status,
        warmup: 0,
        accept,
        body: None,
        ip_off: false,
    }
}

/// An IP endpoint case against the stub lookup, with an optional JSON body.
const fn ip(
    doc_path: &'static str,
    method: &'static str,
    uri: &'static str,
    status: u16,
    body: Option<&'static str>,
) -> Case {
    Case {
        store: Store::Absent,
        doc_path,
        method,
        uri,
        status,
        warmup: 0,
        accept: None,
        body,
        ip_off: false,
    }
}

/// An IP endpoint case with no IP module configured.
const fn ip_off(
    doc_path: &'static str,
    method: &'static str,
    uri: &'static str,
    body: Option<&'static str>,
) -> Case {
    Case {
        ip_off: true,
        ..ip(doc_path, method, uri, 503, body)
    }
}

const TLS_OK: &str = r#"{"host":"example.com"}"#;
const TLS_BLOCKED: &str = r#"{"host":"10.0.0.1"}"#;
const HTTP_OK: &str = r#"{"url":"https://example.com/"}"#;
const HTTP_BAD: &str = r#"{"url":"http://example.com/"}"#;
const EMAIL_OK: &str = r#"{"domain":"example.com"}"#;
const EMAIL_BAD: &str = r#"{"domain":"not a domain"}"#;
const EMAIL: &str = "/api/v1/email/inspect";

/// An email inspection case: the stub inspector answers `example.com` from its zone.
const fn email(status: u16, accept: Option<&'static str>, body: &'static str) -> Case {
    Case {
        accept,
        ..ip(EMAIL, "post", "/email/inspect", status, Some(body))
    }
}

const BATCH: &str = r#"{"addresses":["192.0.2.1","192.0.2.2"]}"#;
const DIFF: &str = r#"{"a":"192.0.2.1","b":"192.0.2.2"}"#;
const DIFF_BAD: &str = r#"{"a":"192.0.2.1","b":"nope"}"#;

fn cases() -> Vec<Case> {
    vec![
        case("/api/v1/catalogue", "get", "/catalogue", 200, 0),
        case("/api/v1/catalogue", "get", "/catalogue", 429, 1),
        case("/api/v1/profiles/{n}", "get", "/profiles/3", 200, 0),
        case("/api/v1/profiles/{n}", "get", "/profiles/99", 404, 0),
        case("/api/v1/profiles/{n}", "get", "/profiles/x", 400, 0),
        case("/api/v1/meta", "get", "/meta", 200, 0),
        case("/api/v1/meta", "get", "/meta", 429, 1),
        seeded("/api/v1/domains/{domain}", "/domains/example.com", 200),
        seeded("/api/v1/domains/{domain}", "/domains/none.example.com", 404),
        seeded("/api/v1/domains/{domain}", "/domains/bad_domain", 400),
        case(
            "/api/v1/domains/{domain}",
            "get",
            "/domains/example.com",
            503,
            0,
        ),
        seeded(
            "/api/v1/domains/{domain}/diff",
            "/domains/example.com/diff",
            200,
        ),
        seeded(
            "/api/v1/domains/{domain}/diff",
            "/domains/first.example.com/diff",
            200,
        ),
        seeded(
            "/api/v1/domains/{domain}/diff",
            "/domains/none.example.com/diff",
            404,
        ),
        seeded(
            "/api/v1/domains/{domain}/history",
            "/domains/example.com/history",
            200,
        ),
        seeded(
            "/api/v1/domains/{domain}/history",
            "/domains/none.example.com/history",
            404,
        ),
        seeded("/api/v1/entries/{id}", "/entries/{e2}", 200),
        seeded("/api/v1/entries/{id}", "/entries/AAAAAAAA", 410),
        seeded("/api/v1/entries/{id}", "/entries/bad", 404),
        case("/api/v1/entries/{id}", "get", "/entries/AAAAAAAA", 503, 0),
        seeded("/api/v1/entries/{id}/diff", "/entries/{e2}/diff", 200),
        seeded("/api/v1/entries/{id}/diff", "/entries/AAAAAAAA/diff", 410),
        post_run("/domains/ok.example.com/runs", 200, None),
        post_run(
            "/domains/ok.example.com/runs",
            200,
            Some("text/event-stream"),
        ),
        post_run("/domains/bad_domain/runs", 400, None),
        post_run("/domains/throttled.example.com/runs", 429, None),
        post_run("/domains/pre.example.com/runs", 503, None),
        ip("/api/v1/ip", "get", "/ip", 200, None),
        ip("/api/v1/ip", "get", "/ip?field=ip", 200, None),
        ip(
            "/api/v1/ip/{address}",
            "get",
            "/ip/198.51.100.1?field=country",
            200,
            None,
        ),
        ip_off("/api/v1/ip", "get", "/ip", None),
        ip("/api/v1/ip/{address}", "get", "/ip/192.0.2.1", 200, None),
        ip("/api/v1/ip/{address}", "get", "/ip/not-an-ip", 400, None),
        ip("/api/v1/ip/batch", "post", "/ip/batch", 200, Some(BATCH)),
        ip_off("/api/v1/ip/batch", "post", "/ip/batch", Some(BATCH)),
        ip(
            "/api/v1/ip/range/{cidr}",
            "get",
            "/ip/range/192.0.2.0/24",
            200,
            None,
        ),
        ip(
            "/api/v1/ip/range/{cidr}",
            "get",
            "/ip/range/10.0.0.0/8",
            400,
            None,
        ),
        ip("/api/v1/ip/diff", "post", "/ip/diff", 200, Some(DIFF)),
        ip("/api/v1/ip/diff", "post", "/ip/diff", 400, Some(DIFF_BAD)),
        ip("/api/v1/asn/{n}", "get", "/asn/64496", 200, None),
        ip("/api/v1/asn/{n}", "get", "/asn/x", 400, None),
        ip(
            "/api/v1/tls/inspect",
            "post",
            "/tls/inspect",
            200,
            Some(TLS_OK),
        ),
        ip(
            "/api/v1/tls/inspect",
            "post",
            "/tls/inspect",
            403,
            Some(TLS_BLOCKED),
        ),
        ip(
            "/api/v1/http/inspect",
            "post",
            "/http/inspect",
            200,
            Some(HTTP_OK),
        ),
        ip(
            "/api/v1/http/inspect",
            "post",
            "/http/inspect",
            400,
            Some(HTTP_BAD),
        ),
        email(200, None, EMAIL_OK),
        email(200, Some("text/event-stream"), EMAIL_OK),
        email(400, None, EMAIL_BAD),
    ]
}

fn run(status: Status, findings: Vec<String>) -> Run {
    let block = BlockId::parse("dns.caa").unwrap();
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status,
            findings,
            evidence: vec![block.clone()],
        }],
        blocks: vec![Block {
            id: block,
            title: "CAA".into(),
            shape: Shape::Text("0 issue \"example.org\"".into()),
        }],
    }
}

async fn record(store: &HistoryStore, domain: &str, run: &Run, now: i64) -> String {
    match store
        .record(domain, run, &[], now, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } | Recorded::Changed { id, .. } => id,
        other => panic!("expected a new entry, got {other:?}"),
    }
}

/// example.com holds e1 (pass) then e2 (warn, with a finding and evidence); first.example.com
/// holds the single entry f1. Returns the store and the uri placeholders' values.
async fn seed() -> (
    Arc<HistoryStore>,
    tempfile::TempDir,
    [(&'static str, String); 3],
) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let t = 1_800_000_000;
    let e1 = record(&store, "example.com", &run(Status::Pass, vec![]), t).await;
    let e2 = record(
        &store,
        "example.com",
        &run(Status::Warn, vec!["CAA does not name the issuer".into()]),
        t + 100,
    )
    .await;
    let f1 = record(&store, "first.example.com", &run(Status::Pass, vec![]), t).await;
    (
        Arc::new(store),
        dir,
        [("{e1}", e1), ("{e2}", e2), ("{f1}", f1)],
    )
}

fn fixture() -> Value {
    let path = format!(
        "{}/../../tests/fixtures/contracts/api-v1.openapi.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("committed document {path} is missing: {e}"));
    serde_json::from_str(&text).expect("committed document is JSON")
}

fn app(store: Option<Arc<HistoryStore>>, ip_off: bool) -> axum::Router {
    let parts = ApiParts {
        ip: (!ip_off).then(|| Arc::new(StubIpLookup::new()) as Arc<dyn netray_ip::IpLookup>),
        tls: Some(stub_state(Arc::new(StubTlsBackend::new())).inspector()),
        http: Some(Arc::new(StubHttpInspect::new())),
        email: Some(Arc::new(netray_email::testing::stub_inspector(
            netray_email::testing::example_dns(),
        ))),
    };
    let config = ApiConfig {
        per_ip_burst: 1,
        ..Default::default()
    };
    router(
        ApiState::new(config, Arc::new(StubRuns))
            .with_store(store)
            .with_parts(parts),
    )
}

fn request(method: &str, uri: &str) -> Request<Body> {
    request_accepting(method, uri, None, None)
}

fn request_accepting(
    method: &str,
    uri: &str,
    accept: Option<&str>,
    body: Option<&str>,
) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::from_bytes(method.to_uppercase().as_bytes()).unwrap())
        .uri(uri);
    if let Some(a) = accept {
        b = b.header("accept", a);
    }
    match body {
        Some(json) => b
            .header("content-type", "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

/// Validates `body` against the named `components.schemas` entry of the document.
fn validate_component(doc: &Value, name: &str, body: &Value, label: &str) {
    let mut root = doc["components"]["schemas"][name].clone();
    assert!(root.is_object(), "{label}: component {name} not documented");
    root.as_object_mut()
        .unwrap()
        .insert("components".into(), doc["components"].clone());
    let validator = jsonschema::validator_for(&root)
        .unwrap_or_else(|e| panic!("{label}: {name} does not compile: {e}"));
    let errors: Vec<String> = validator.iter_errors(body).map(|e| e.to_string()).collect();
    assert!(
        errors.is_empty(),
        "{label}: body violates {name}: {errors:?}\nbody: {body}"
    );
}

/// The `(event, data)` pairs of an SSE body.
fn sse_events(text: &str) -> Vec<(String, String)> {
    text.split("\n\n")
        .filter(|b| !b.trim().is_empty())
        .map(|block| {
            let mut event = String::new();
            let mut data = Vec::new();
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("event:") {
                    event = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("data:") {
                    data.push(v.strip_prefix(' ').unwrap_or(v).to_string());
                }
            }
            (event, data.join("\n"))
        })
        .collect()
}

#[tokio::test]
async fn responses_match_the_committed_document() {
    let doc = fixture();
    for c in cases() {
        let label = format!("{} {} -> {}", c.method, c.doc_path, c.status);
        let mut uri = c.uri.to_string();
        let (app, _dir) = match c.store {
            Store::Absent => (app(None, c.ip_off), None),
            Store::Seeded => {
                let (store, dir, ids) = seed().await;
                for (placeholder, id) in &ids {
                    uri = uri.replace(placeholder, id);
                }
                (app(Some(store), false), Some(dir))
            }
        };
        for _ in 0..c.warmup {
            app.clone().oneshot(request(c.method, &uri)).await.unwrap();
        }
        let resp = app
            .oneshot(request_accepting(c.method, &uri, c.accept, c.body))
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), c.status, "{label}: status");

        let documented = &doc["paths"][c.doc_path][c.method]["responses"][c.status.to_string()];
        assert!(documented.is_object(), "{label}: status not documented");

        let content_type = resp
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let content = &documented["content"];
        if c.status == 429 && c.doc_path == RUNS {
            assert!(
                resp.headers().contains_key("retry-after"),
                "{label}: no Retry-After header"
            );
        }
        if c.accept == Some("text/event-stream") {
            assert!(
                content.get("text/event-stream").is_some(),
                "{label}: text/event-stream not documented"
            );
            assert!(
                content_type.starts_with("text/event-stream"),
                "{label}: content type {content_type}"
            );
            let bytes = resp.into_body().collect().await.unwrap().to_bytes();
            let events = sse_events(std::str::from_utf8(&bytes).unwrap());
            if c.doc_path == EMAIL {
                // beacon's events carry no SSE event name: the `type` field of the data tags them.
                assert!(
                    events.iter().all(|(e, _)| e.is_empty()),
                    "{label}: email events carry event names: {events:?}"
                );
                let schema = &content["text/event-stream"]["schema"];
                assert!(schema.is_object(), "{label}: no event-stream schema");
                let mut root = schema.clone();
                root.as_object_mut()
                    .unwrap()
                    .insert("components".into(), doc["components"].clone());
                let validator = jsonschema::validator_for(&root)
                    .unwrap_or_else(|e| panic!("{label}: schema does not compile: {e}"));
                let mut types = Vec::new();
                for (_, data) in &events {
                    let body: Value = serde_json::from_str(data)
                        .unwrap_or_else(|e| panic!("{label}: data is not JSON: {e}"));
                    let errors: Vec<String> = validator
                        .iter_errors(&body)
                        .map(|e| e.to_string())
                        .collect();
                    assert!(
                        errors.is_empty(),
                        "{label}: event violates schema: {errors:?}\n{body}"
                    );
                    types.push(body["type"].as_str().unwrap_or_default().to_string());
                }
                let n = types.len();
                assert!(n >= 3, "{label}: type sequence {types:?}");
                assert!(
                    types[..n - 2].iter().all(|t| t == "category"),
                    "{label}: type sequence {types:?}"
                );
                assert_eq!(
                    &types[n - 2..],
                    ["evidence", "summary"],
                    "{label}: type sequence"
                );
                continue;
            }
            let names: Vec<&str> = events.iter().map(|(e, _)| e.as_str()).collect();
            assert_eq!(names, ["section", "done"], "{label}: event order");
            for (event, data) in &events {
                let body: Value = serde_json::from_str(data)
                    .unwrap_or_else(|e| panic!("{label}: {event} data is not JSON: {e}"));
                let component = if event == "section" {
                    "RunSectionEvent"
                } else {
                    "RunAnswer"
                };
                validate_component(&doc, component, &body, &format!("{label} {event}"));
            }
            continue;
        }
        // The response's own media type must be one of the document's types for this status.
        let media = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        assert!(
            content.get(media.as_str()).is_some(),
            "{label}: response media type {media:?} not documented (documented: {:?})",
            content.as_object().map(|o| o.keys().collect::<Vec<_>>())
        );
        if media == "text/plain" {
            continue;
        }
        let schema = &content[media.as_str()]["schema"];
        assert!(schema.is_object(), "{label}: no {media} schema documented");

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("{label}: body is not JSON: {e}"));

        // $refs are "#/components/schemas/X": graft the fixture's components onto the schema root.
        let mut root = schema.clone();
        root.as_object_mut()
            .unwrap()
            .insert("components".into(), doc["components"].clone());
        let validator = jsonschema::validator_for(&root)
            .unwrap_or_else(|e| panic!("{label}: schema does not compile: {e}"));
        let errors: Vec<String> = validator
            .iter_errors(&body)
            .map(|e| e.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "{label}: body violates schema: {errors:?}\nbody: {body}"
        );
    }
}

const METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// One message per path+method of `doc` lacking a 2xx case or a non-2xx case in `cases`.
fn uncovered(doc: &Value, cases: &[Case]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(paths) = doc["paths"].as_object() else {
        return vec!["document has no paths".into()];
    };
    for (path, item) in paths {
        for method in METHODS.iter().filter(|m| item.get(**m).is_some()) {
            let mine = || {
                cases
                    .iter()
                    .filter(|c| c.doc_path == path && c.method == *method)
            };
            if !mine().any(|c| (200..300).contains(&c.status)) {
                out.push(format!("{path} {method}: no 2xx case"));
            }
            if !mine().any(|c| !(200..300).contains(&c.status)) {
                out.push(format!("{path} {method}: no non-2xx case"));
            }
        }
    }
    out
}

#[test]
fn every_path_and_method_has_a_success_and_an_error_case() {
    let missing = uncovered(&fixture(), &cases());
    assert!(missing.is_empty(), "uncovered: {missing:?}");
}

#[test]
fn coverage_check_names_an_uncovered_path() {
    let mut doc = fixture();
    doc["paths"]["/api/v1/ghost"] = serde_json::json!({ "get": { "responses": {} } });
    let missing = uncovered(&doc, &cases());
    assert!(
        missing.iter().any(|m| m.contains("/api/v1/ghost")),
        "ghost path not reported: {missing:?}"
    );
}
