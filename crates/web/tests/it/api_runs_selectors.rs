//! v2-dkim-legacy-urls Phase 2: `POST /domains/{domain}/runs?dkim_selectors=<list>` parses and
//! validates the selector list, and hands it to `DomainRuns::start_with`.
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_web::api::{
    ApiConfig, ApiState, DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger,
};
use serde_json::Value;
use tower::ServiceExt;

type Call = (String, Vec<String>);

struct StubRuns {
    calls: Mutex<Vec<Call>>,
}

fn admitted() -> RunStart {
    RunStart::Admitted(Box::pin(futures::stream::iter(Vec::<RunEvent>::new())))
}

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.trim().to_lowercase())
    }
    fn start<'a>(
        &'a self,
        domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        self.calls
            .lock()
            .unwrap()
            .push((domain.to_string(), vec![]));
        Box::pin(async move { admitted() })
    }
    fn start_with<'a>(
        &'a self,
        domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
        dkim_selectors: &'a [String],
    ) -> BoxFuture<'a, RunStart> {
        self.calls
            .lock()
            .unwrap()
            .push((domain.to_string(), dkim_selectors.to_vec()));
        Box::pin(async move { admitted() })
    }
}

fn setup() -> (Router, Arc<StubRuns>) {
    let stub = Arc::new(StubRuns {
        calls: Mutex::new(vec![]),
    });
    let app = netray_web::api::router(ApiState::new(ApiConfig::default(), stub.clone()));
    (app, stub)
}

async fn post(app: &Router, path: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(Request::post(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn post_runs_dkim_selectors_query_is_parsed_validated_and_passed_on() {
    let a63 = "a".repeat(63);
    let a64 = "a".repeat(64);
    let cases: Vec<(&str, String, Option<Vec<String>>)> = vec![
        (
            "list with space",
            "google,%20s1".into(),
            Some(strs(&["google", "s1"])),
        ),
        ("dot", "bad.sel".into(), None),
        ("six tokens", "a,b,c,d,e,f".into(), None),
        (
            "five tokens",
            "a,b,c,d,e".into(),
            Some(strs(&["a", "b", "c", "d", "e"])),
        ),
        ("empty token", "a,,b".into(), None),
        ("64 chars", a64, None),
        ("63 chars", a63.clone(), Some(vec![a63])),
        (
            "hyphen and digits",
            "s-1,k2".into(),
            Some(strs(&["s-1", "k2"])),
        ),
        ("underscore", "a_b".into(), None),
        ("empty value", "".into(), Some(vec![])),
        ("whitespace only", "%20%20".into(), Some(vec![])),
    ];
    for (name, query, want) in cases {
        let (app, stub) = setup();
        let (status, body) = post(
            &app,
            &format!("/domains/example.com/runs?dkim_selectors={query}"),
        )
        .await;
        let calls = stub.calls.lock().unwrap();
        match want {
            Some(selectors) => {
                assert_eq!(status, StatusCode::OK, "{name}: {body}");
                assert_eq!(
                    *calls,
                    vec![("example.com".to_string(), selectors)],
                    "{name}"
                );
            }
            None => {
                assert_eq!(status, StatusCode::BAD_REQUEST, "{name}: {body}");
                assert_eq!(body["error"]["code"], "INVALID_INPUT", "{name}");
                assert!(calls.is_empty(), "{name}: stub was called");
            }
        }
    }
}

#[tokio::test]
async fn post_runs_without_dkim_selectors_starts_with_an_empty_list() {
    let (app, stub) = setup();
    let (status, body) = post(&app, "/domains/example.com/runs").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        *stub.calls.lock().unwrap(),
        vec![("example.com".to_string(), Vec::<String>::new())]
    );
}

#[test]
fn openapi_documents_the_dkim_selectors_query_parameter_and_array_body_field() {
    let doc = serde_json::to_value(netray_web::api::openapi()).unwrap();
    let params = doc["paths"]["/api/v1/domains/{domain}/runs"]["post"]["parameters"]
        .as_array()
        .expect("post parameters");
    let p = params
        .iter()
        .find(|p| p["name"] == "dkim_selectors")
        .expect("dkim_selectors parameter");
    assert_eq!(p["in"], "query");
    assert!(
        p.get("required").is_none_or(|r| r == &Value::Bool(false)),
        "{p}"
    );
    assert_eq!(
        doc["components"]["schemas"]["EmailInspectRequest"]["properties"]["dkim_selectors"]["type"],
        "array"
    );
}
