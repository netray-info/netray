//! API v1 edge behaviour (spec v2-api): method refusals, invalid path input, the profile builder.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::profile::{Profile, Profiles};
use netray_web::api::{ApiConfig, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger};
use serde_json::Value;
use tower::ServiceExt;

struct StubRuns;

impl DomainRuns for StubRuns {
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

fn app() -> Router {
    netray_web::api::router(ApiState::new(ApiConfig::default(), Arc::new(StubRuns)))
}

async fn send(app: Router, req: Request<Body>) -> (StatusCode, Option<String>, Value) {
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        ct,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn assert_json_error(ct: &Option<String>, body: &Value, code: &str, label: &str) {
    assert!(
        ct.as_deref()
            .is_some_and(|c| c.starts_with("application/json")),
        "{label}: content-type {ct:?}"
    );
    assert_eq!(body["error"]["code"], code, "{label}: {body}");
}

#[tokio::test]
async fn wrong_method_answers_405_with_the_json_error_shape() {
    for (method, path) in [
        ("POST", "/catalogue"),
        ("DELETE", "/meta"),
        ("POST", "/openapi.json"),
    ] {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let (s, ct, body) = send(app(), req).await;
        assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED, "{method} {path}");
        assert_json_error(
            &ct,
            &body,
            "METHOD_NOT_ALLOWED",
            &format!("{method} {path}"),
        );
    }
}

#[tokio::test]
async fn profile_path_edge_cases_answer_json_errors() {
    for (path, status, code) in [
        ("/profiles/%FF", StatusCode::BAD_REQUEST, "INVALID_INPUT"),
        ("/profiles/-1", StatusCode::BAD_REQUEST, "INVALID_INPUT"),
        ("/profiles/+3", StatusCode::BAD_REQUEST, "INVALID_INPUT"),
        ("/profiles/4294967296", StatusCode::NOT_FOUND, "NOT_FOUND"),
    ] {
        let (s, ct, body) = send(app(), Request::get(path).body(Body::empty()).unwrap()).await;
        assert_eq!(s, status, "{path}");
        assert_json_error(&ct, &body, code, path);
    }
}

fn profile_v4() -> Arc<Profile> {
    let text = include_str!("../../engine/profiles/v3.toml");
    assert!(text.contains("version = 3"));
    Arc::new(Profile::from_toml(&text.replacen("version = 3", "version = 4", 1)).unwrap())
}

#[tokio::test]
async fn with_profile_sets_the_graded_profile_in_meta_and_profiles() {
    let state = ApiState::new(ApiConfig::default(), Arc::new(StubRuns)).with_profile(profile_v4());
    let app = netray_web::api::router(state);

    let (s, _, meta) = send(
        app.clone(),
        Request::get("/meta").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(meta["current_profile"], 4);
    let versions: Vec<u64> = meta["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    assert!(
        versions.contains(&3) && versions.contains(&4),
        "{versions:?}"
    );

    let (s, _, p) = send(
        app,
        Request::get("/profiles/4").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(p["version"], 4);
}

#[tokio::test]
async fn without_with_profile_current_profile_is_the_embedded_current() {
    let (_, _, meta) = send(app(), Request::get("/meta").body(Body::empty()).unwrap()).await;
    assert_eq!(
        meta["current_profile"],
        u64::from(Profiles::current().version)
    );
}
