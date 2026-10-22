//! spec v2-result-page, requirement 2 (C2, C14): the result router's domain segment handling.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

struct Stub;

impl DomainRuns for Stub {
    fn validate(&self, segment: &str) -> Result<String, DomainRefusal> {
        if segment.parse::<IpAddr>().is_ok() {
            return Err(DomainRefusal {
                code: "DOMAIN_INVALID",
                message: "an IP address is not a domain".into(),
            });
        }
        let s = segment.to_lowercase();
        Ok(s.strip_suffix('.').unwrap_or(&s).to_string())
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

async fn app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let state = ResultState::new(Arc::new(store), Arc::new(Stub), 3600);
    (router(state), dir)
}

async fn send(method: &str, path: &str) -> (StatusCode, Option<String>, String) {
    let (app, _dir) = app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let location = resp
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string());
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, location, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn refused_segment_answers_400_not_found_page_with_reason_and_input_refilled() {
    let (status, _, body) = send("GET", "/1.2.3.4").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.contains("<h1>Nothing here</h1>"),
        "not-found template: {body}"
    );
    assert!(
        body.contains("an IP address is not a domain"),
        "reason: {body}"
    );
    assert!(
        body.contains(r#"value="1.2.3.4""#),
        "input refilled: {body}"
    );
}

#[tokio::test]
async fn normalised_difference_redirects_get_and_rejects_post() {
    let cases = [
        (
            "GET",
            "/Shop.Example.com.",
            StatusCode::MOVED_PERMANENTLY,
            Some("/shop.example.com"),
        ),
        (
            "GET",
            "/shop.example.com./history",
            StatusCode::MOVED_PERMANENTLY,
            Some("/shop.example.com/history"),
        ),
        (
            "POST",
            "/Shop.Example.com/run",
            StatusCode::BAD_REQUEST,
            None,
        ),
    ];
    for (method, path, want_status, want_location) in cases {
        let (status, location, _) = send(method, path).await;
        assert_eq!(status, want_status, "{method} {path}");
        assert_eq!(location.as_deref(), want_location, "{method} {path}");
    }
}
