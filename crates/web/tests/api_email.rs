//! API v1 email inspection (spec v2-api, Phase 6): `POST /email/inspect` driven through
//! `netray_web::api::router` over a stub DNS backend, paths relative to the mount.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunSection, SectionKind};
use netray_model::{Grade, Protocol};
use netray_store::{HistoryStore, Recorded};
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

fn stub_app(store: Option<Arc<HistoryStore>>) -> Router {
    let parts = ApiParts {
        email: Some(Arc::new(netray_email::testing::stub_inspector(
            netray_email::testing::example_dns(),
        ))),
        ..Default::default()
    };
    router(
        ApiState::new(ApiConfig::default(), Arc::new(NoRuns))
            .with_store(store)
            .with_parts(parts),
    )
}

struct Reply {
    status: StatusCode,
    content_type: String,
    body: String,
}

async fn post(app: Router, accept: Option<&str>, body: Value) -> Reply {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/email/inspect")
        .header("content-type", "application/json");
    if let Some(a) = accept {
        builder = builder.header("accept", a);
    }
    let mut req = builder.body(Body::from(body.to_string())).unwrap();
    let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.oneshot(req).await.unwrap();
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

/// Validates `value` against a documented schema (path-relative `content` entry) with the
/// document's components grafted in.
fn validate(schema_of: impl Fn(&Value) -> Value, value: &Value) {
    let doc = serde_json::to_value(openapi()).unwrap();
    let mut root = schema_of(&doc);
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

fn validate_result(body: &Value) {
    validate(
        |doc| {
            doc["paths"]["/api/v1/email/inspect"]["post"]["responses"]["200"]["content"]["application/json"]
                ["schema"]
                .clone()
        },
        body,
    );
}

fn validate_event(event: &Value) {
    validate(
        |doc| {
            doc["paths"]["/api/v1/email/inspect"]["post"]["responses"]["200"]["content"]["text/event-stream"]
                ["schema"]
                .clone()
        },
        event,
    );
}

async fn seeded_store() -> (Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let run = Run {
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
        checks: vec![],
        blocks: vec![],
    };
    let recorded = store
        .record("example.com", &run, &[], 1_800_000_000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    assert!(matches!(recorded, Recorded::First { .. }), "{recorded:?}");
    (Arc::new(store), dir)
}

/// C2: Accept JSON answers one result object, valid against the document; nothing is stored.
#[tokio::test]
async fn json_answer_is_valid_and_stores_nothing() {
    let (store, _dir) = seeded_store().await;
    let before = store.entries("example.com").await.unwrap().len();
    assert_eq!(before, 1);

    let r = post(
        stub_app(Some(store.clone())),
        Some("application/json"),
        json!({"domain": "example.com"}),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("application/json"),
        "{}",
        r.content_type
    );
    let body = json_of(&r);
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(keys, ["categories", "evidence", "summary"], "{body}");
    assert_eq!(body["categories"].as_array().unwrap().len(), 12, "{body}");
    validate_result(&body);

    assert_eq!(store.entries("example.com").await.unwrap().len(), before);
}

/// C2: no Accept header answers JSON too.
#[tokio::test]
async fn no_accept_answers_json() {
    let r = post(stub_app(None), None, json!({"domain": "example.com"})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("application/json"),
        "{}",
        r.content_type
    );
    validate_result(&json_of(&r));
}

/// C3: Accept event-stream answers beacon's frames: no event name, category x12, evidence, summary.
#[tokio::test]
async fn event_stream_answers_categories_evidence_summary() {
    let r = post(
        stub_app(None),
        Some("text/event-stream"),
        json!({"domain": "example.com"}),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("text/event-stream"),
        "{}",
        r.content_type
    );

    let mut types = Vec::new();
    for block in r.body.split("\n\n").filter(|b| !b.trim().is_empty()) {
        let mut data = String::new();
        for line in block.lines() {
            assert!(!line.starts_with("event:"), "named event: {block}");
            if let Some(d) = line.strip_prefix("data:") {
                data.push_str(d.trim_start());
            }
        }
        let event: Value = serde_json::from_str(&data).unwrap();
        validate_event(&event);
        types.push(event["type"].as_str().unwrap().to_string());
    }
    let mut expected = vec!["category"; 12];
    expected.push("evidence");
    expected.push("summary");
    assert_eq!(types, expected);
}

/// C5: a literal IP answers 200; an unparsable domain and an unknown field answer 400.
#[tokio::test]
async fn input_is_checked() {
    let r = post(stub_app(None), None, json!({"domain": "192.0.2.1"})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);

    let cases = [
        json!({"domain": "not a domain"}),
        json!({"domain": "example.com", "dkim_selectors": ["s1"]}),
    ];
    for case in cases {
        let r = post(stub_app(None), None, case.clone()).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{case}: {}", r.body);
        assert_eq!(json_of(&r)["error"]["code"], "INVALID_INPUT", "{case}");
    }
}
