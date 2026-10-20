//! V2 spec v2-site-nav, Phase 6 (requirements 16, 17; C1, C3, C4, C5, C6): a decoded domain is
//! shown in Unicode with its A-label beside it; URLs, canonical, og tags and the API keep the A-label.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{Entry, HistoryStore, Recorded};
use netray_web::api::{ApiConfig, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::entries::render;
use netray_web::entries::view::entry_view;
use netray_web::pages::result::{ResultState, router};
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

const A: &str = "xn--bcher-kva.example";
const SPAN: &str = "<span class=\"idn-alabel\">xn--bcher-kva.example</span>";

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.trim().to_lowercase())
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

fn run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.30.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Email,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("email.mx").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// A store holding one fresh entry for `domain`; returns the store, the entry and the dir guard.
async fn stored(domain: &str) -> (Arc<HistoryStore>, Entry, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let id = match store
        .record(domain, &run(), &[], now() - 10, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
    let entry = store.latest(domain).await.unwrap().unwrap();
    assert_eq!(entry.id, id);
    (Arc::new(store), entry, dir)
}

async fn get(app: &Router, path: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn result_app(store: &Arc<HistoryStore>) -> Router {
    router(ResultState::new(store.clone(), Arc::new(StubRuns), 3600))
}

/// The text of the first `<h1>`, markup included.
fn h1(html: &str) -> &str {
    let rest = html.split("<h1").nth(1).expect("an h1");
    let rest = &rest[rest.find('>').unwrap() + 1..];
    rest.split("</h1>").next().unwrap()
}

#[test]
fn c3_unicode_decodes_an_a_label_and_gives_nothing_otherwise() {
    assert_eq!(
        netray_web::idn::unicode("xn--bcher-kva.example"),
        Some("bücher.example".to_string())
    );
    assert_eq!(netray_web::idn::unicode("example.com"), None);
    assert_eq!(netray_web::idn::unicode("xn--zz.example"), None);
}

#[tokio::test]
async fn c4_result_page_shows_both_forms_and_keeps_the_a_label_in_urls() {
    let (store, _entry, _dir) = stored(A).await;
    let (status, body) = get(&result_app(&store), &format!("/{A}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let head = h1(&body);
    assert!(
        head.contains("<span class=\"result-hero__label\">bücher.</span>"),
        "{head}"
    );
    assert!(
        head.contains("<span class=\"result-hero__label\">example</span>"),
        "{head}"
    );
    assert!(head.contains(&format!(" {SPAN}")), "{head}");
    assert!(
        body.contains("<title>bücher.example (xn--bcher-kva.example)"),
        "{body}"
    );
    assert!(
        body.contains("https://netray.info/xn--bcher-kva.example"),
        "{body}"
    );
    for tag in body.split('<').filter(|t| {
        t.starts_with("link rel=\"canonical\"") || t.starts_with("meta property=\"og:url\"")
    }) {
        assert!(tag.contains(A), "{tag}");
        assert!(!tag.contains("bücher"), "unicode in {tag}");
    }
    assert!(body.contains(&format!("data-domain=\"{A}\"")), "{body}");
    assert!(body.contains("data-display=\"bücher.example\""), "{body}");
}

#[tokio::test]
async fn c1_a_domain_without_a_unicode_form_shows_no_a_label_and_no_display() {
    let (store, _entry, _dir) = stored("example.com").await;
    let app = result_app(&store);
    for path in ["/example.com", "/example.com/history"] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        assert!(!body.contains("idn-alabel"), "{path}: {body}");
        assert!(!body.contains("data-display"), "{path}: {body}");
    }
}

#[tokio::test]
async fn c1_history_heading_shows_both_forms() {
    let (store, _entry, _dir) = stored(A).await;
    let (status, body) = get(&result_app(&store), &format!("/{A}/history")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(h1(&body), format!("bücher.example {SPAN}"));
}

#[tokio::test]
async fn c5_evidence_view_heading_shows_both_forms() {
    let (store, _entry, _dir) = stored(A).await;
    let app = evidence_router(EvidenceDeps {
        history: store,
        v1: None,
    });
    let (status, body) = get(&app, &format!("/{A}/email")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(h1(&body), format!("bücher.example {SPAN} · email"));
}

#[tokio::test]
async fn c5_entry_page_heading_shows_both_forms() {
    let (_store, entry, _dir) = stored(A).await;
    let html = render::entry(&entry_view(&entry)).expect("render entry");
    assert_eq!(h1(&html), format!("bücher.example {SPAN}"));
}

#[tokio::test]
async fn c1_the_api_keeps_the_a_label_only() {
    let (store, _entry, _dir) = stored(A).await;
    let app = netray_web::api::router(
        ApiState::new(ApiConfig::default(), Arc::new(StubRuns)).with_store(Some(store)),
    );
    let (status, body) = get(&app, &format!("/domains/{A}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(A), "{body}");
    assert!(!body.contains("bücher"), "{body}");
}
