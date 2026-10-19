//! The idle page (spec v2-result-page, Phase 2, requirement 24; C9, C18): `GET /` answers the
//! query form, the hidden recent list, examples, tagline and inspector links; `GET /?d=` redirects.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        let d = domain.trim().to_lowercase();
        Ok(d.strip_suffix('.').unwrap_or(&d).to_string())
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
    let state = ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600);
    (router(state), dir)
}

async fn get(app: &Router, path: &str) -> (StatusCode, Option<String>, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let location = resp
        .headers()
        .get(header::LOCATION)
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        location,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

/// The element opening tag that contains `needle`, or panic.
fn tag_with<'a>(body: &'a str, needle: &str) -> &'a str {
    let at = body
        .find(needle)
        .unwrap_or_else(|| panic!("no {needle} in {body}"));
    let start = body[..at].rfind('<').unwrap();
    let end = at + body[at..].find('>').unwrap();
    &body[start..=end]
}

#[tokio::test]
async fn c9_idle_page_elements() {
    let (app, _dir) = app().await;

    let (status, _, body) = get(&app, "/").await;

    assert_eq!(status, StatusCode::OK);
    let form = tag_with(&body, "action=\"/\"");
    assert!(form.starts_with("<form"), "{form}");
    assert!(body.contains("name=\"d\""), "{body}");
    let recent = tag_with(&body, "id=\"recent\"");
    assert!(recent.contains("data-island=\"recent\""), "{recent}");
    assert!(recent.contains("hidden"), "{recent}");
    assert!(body.contains("href=\"/example.com\""), "{body}");
    assert!(body.contains("href=\"/netray.info\""), "{body}");
    let main_start = body.find("<main").expect("no <main>");
    let main_end = body.find("</main>").expect("no </main>");
    let main = &body[main_start..main_end];
    assert!(
        main.contains("Every check of a domain, in one spectrum."),
        "tagline missing from main: {main}"
    );
    for link in [
        "/inspect/dns",
        "/inspect/tls",
        "/inspect/http",
        "/inspect/ip",
    ] {
        assert!(
            body.contains(&format!("href=\"{link}\"")),
            "{link} in {body}"
        );
    }
}

#[tokio::test]
async fn c18_query_redirects_to_the_normalised_domain() {
    let (app, _dir) = app().await;

    let (status, location, _) = get(&app, "/?d=Example.com").await;

    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(location.as_deref(), Some("/example.com"));
}
