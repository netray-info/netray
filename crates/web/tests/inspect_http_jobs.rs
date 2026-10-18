//! V2 spec v2-inspectors, requirement 16 (C2) and the HTTP URL scenario (C11): the HTTP inspector
//! job accepts any http/https URL, path and query kept, renders the translation's blocks and band;
//! an invalid URL is one fragment and the backend is never called.
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_http::HttpInspect;
use netray_http::error::AppError;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::testing::StubHttpInspect;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use tower::ServiceExt;
use url::Url;

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

/// Delegates to the stub and records every URL the job hands to the backend.
struct Recording {
    inner: StubHttpInspect,
    targets: Mutex<Vec<String>>,
    inspects: Mutex<Vec<String>>,
}

impl HttpInspect for Recording {
    fn target<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<SocketAddr, AppError>> {
        self.targets.lock().unwrap().push(url.to_string());
        self.inner.target(url)
    }

    fn inspect<'a>(
        &'a self,
        url: &'a Url,
        target: SocketAddr,
        start: Instant,
    ) -> BoxFuture<'a, Result<InspectResponse, AppError>> {
        self.inspects.lock().unwrap().push(url.to_string());
        self.inner.inspect(url, target, start)
    }
}

fn app() -> (Router, Arc<Recording>) {
    let rec = Arc::new(Recording {
        inner: StubHttpInspect::new(),
        targets: Mutex::new(Vec::new()),
        inspects: Mutex::new(Vec::new()),
    });
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(ApiParts {
        http: Some(rec.clone()),
        ..Default::default()
    });
    let deps = InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default());
    (inspect_router(deps), rec)
}

/// (event name, fragment id, html) per event.
async fn post(app: &Router, body: &str) -> (StatusCode, Vec<(String, String, String)>) {
    let resp = app
        .clone()
        .oneshot(
            Request::post("/inspect/http/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut events = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let mut name = String::new();
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim().to_owned();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let (id, html) = match serde_json::from_str::<serde_json::Value>(&data) {
            Ok(j) => (
                j["id"].as_str().unwrap_or_default().to_owned(),
                j["html"].as_str().unwrap_or_default().to_owned(),
            ),
            Err(_) => (String::new(), String::new()),
        };
        events.push((name, id, html));
    }
    (status, events)
}

#[tokio::test]
async fn c2_http_inspector_renders_the_translation_blocks_and_band() {
    let (app, _) = app();

    let (status, events) = post(&app, "url=https%3A%2F%2Fexample.com%2Fpath").await;

    assert_eq!(status, StatusCode::OK);
    let all: String = events.iter().map(|e| e.2.as_str()).collect();
    assert!(all.contains(r#"id="ev-http.headers""#), "{all}");
    assert!(all.contains(r#"id="ev-http."#), "{all}");
    let bands: Vec<&(String, String, String)> =
        events.iter().filter(|e| e.1 == "inspect-band").collect();
    assert!(!bands.is_empty(), "no inspect-band fragment: {events:?}");
    let band = &bands.last().unwrap().2;
    assert!(band.contains(r#"data-code="http."#), "{band}");
    for other in ["tls.", "ip.", "dns.", "email.", "lens."] {
        assert!(
            !band.contains(&format!(r#"data-code="{other}"#)),
            "{other} in band: {band}"
        );
    }
    assert_eq!(events.last().unwrap().0, "end");
}

#[tokio::test]
async fn c2_invalid_url_is_one_fragment_and_the_backend_is_not_called() {
    let (app, rec) = app();

    let (_, events) = post(&app, "url=ftp%3A%2F%2Fexample.com%2F").await;

    let names: Vec<&str> = events.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(names, ["fragment", "end"], "{events:?}");
    assert_eq!(events[0].1, "inspect-result");
    assert!(
        events[0].2.to_lowercase().contains("http"),
        "validation message names the scheme rule: {}",
        events[0].2
    );
    assert!(rec.targets.lock().unwrap().is_empty());
    assert!(rec.inspects.lock().unwrap().is_empty());
}

#[tokio::test]
async fn c11_the_job_receives_the_url_unchanged_with_path_and_query() {
    let (app, rec) = app();

    let (_, events) = post(&app, "url=https%3A%2F%2Fexample.com%2Fpath%3Fx%3D1").await;

    assert_eq!(events.last().unwrap().0, "end");
    assert_eq!(
        *rec.targets.lock().unwrap(),
        ["https://example.com/path?x=1"]
    );
    assert_eq!(
        *rec.inspects.lock().unwrap(),
        ["https://example.com/path?x=1"]
    );
}
