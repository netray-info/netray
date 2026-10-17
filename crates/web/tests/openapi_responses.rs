//! v2-api requirement 6: every case's status is documented in the committed document and its
//! body validates against the documented schema; requirement 6's coverage clause (C23).
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request};
use http_body_util::BodyExt;
use netray_web::api::{
    ApiConfig, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, router,
};
use serde_json::Value;
use tower::ServiceExt;

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: std::net::IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = RunStart> + Send + 'a>> {
        Box::pin(async { RunStart::NotStarted })
    }
}

/// One validated request: `doc_path` is the document key (full path template), `uri` the request
/// path relative to the mount, `warmup` the requests sent first from the same client.
struct Case {
    doc_path: &'static str,
    method: &'static str,
    uri: &'static str,
    status: u16,
    warmup: usize,
}

const fn case(
    doc_path: &'static str,
    method: &'static str,
    uri: &'static str,
    status: u16,
    warmup: usize,
) -> Case {
    Case { doc_path, method, uri, status, warmup }
}

fn cases() -> Vec<Case> {
    vec![
        case("/api/v1/catalogue", "get", "/catalogue", 200, 0),
        case("/api/v1/catalogue", "get", "/catalogue", 429, 1),
        case("/api/v1/profiles/{n}", "get", "/profiles/3", 200, 0),
        case("/api/v1/profiles/{n}", "get", "/profiles/99", 404, 0),
        case("/api/v1/profiles/{n}", "get", "/profiles/x", 400, 0),
        case("/api/v1/meta", "get", "/meta", 200, 0),
        case("/api/v1/meta", "get", "/meta", 429, 1),
    ]
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

fn app() -> axum::Router {
    let config = ApiConfig { per_ip_burst: 1, ..Default::default() };
    router(ApiState::new(config, Arc::new(StubRuns)))
}

fn request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::from_bytes(method.to_uppercase().as_bytes()).unwrap())
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn responses_match_the_committed_document() {
    let doc = fixture();
    for c in cases() {
        let label = format!("{} {} -> {}", c.method, c.doc_path, c.status);
        let app = app();
        for _ in 0..c.warmup {
            app.clone().oneshot(request(c.method, c.uri)).await.unwrap();
        }
        let resp = app.oneshot(request(c.method, c.uri)).await.unwrap();
        assert_eq!(resp.status().as_u16(), c.status, "{label}: status");

        let documented = &doc["paths"][c.doc_path][c.method]["responses"][c.status.to_string()];
        assert!(documented.is_object(), "{label}: status not documented");

        let content_type = resp
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let content = &documented["content"];
        if content.get("text/plain").is_some() {
            assert!(content_type.starts_with("text/plain"), "{label}: content type {content_type}");
            continue;
        }
        let schema = &content["application/json"]["schema"];
        assert!(schema.is_object(), "{label}: no application/json schema documented");
        assert!(content_type.starts_with("application/json"), "{label}: content type {content_type}");

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("{label}: body is not JSON: {e}"));

        // $refs are "#/components/schemas/X": graft the fixture's components onto the schema root.
        let mut root = schema.clone();
        root.as_object_mut().unwrap().insert("components".into(), doc["components"].clone());
        let validator = jsonschema::validator_for(&root)
            .unwrap_or_else(|e| panic!("{label}: schema does not compile: {e}"));
        let errors: Vec<String> = validator.iter_errors(&body).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{label}: body violates schema: {errors:?}\nbody: {body}");
    }
}

const METHODS: [&str; 8] = ["get", "put", "post", "delete", "options", "head", "patch", "trace"];

/// One message per path+method of `doc` lacking a 2xx case or a non-2xx case in `cases`.
fn uncovered(doc: &Value, cases: &[Case]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(paths) = doc["paths"].as_object() else {
        return vec!["document has no paths".into()];
    };
    for (path, item) in paths {
        for method in METHODS.iter().filter(|m| item.get(**m).is_some()) {
            let mine = || cases.iter().filter(|c| c.doc_path == path && c.method == *method);
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
