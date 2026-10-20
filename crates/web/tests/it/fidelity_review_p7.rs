//! Result fidelity Phase 7, review findings: a page view never fetches a badge, because the share
//! previews carry `data-src` and no `src` until the share island opens the dialog.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "example.com";

struct Idle;

impl DomainRuns for Idle {
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
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.caa").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

/// An app whose store holds one entry `age` seconds old, or none.
async fn app(age: Option<i64>) -> Router {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    std::mem::forget(dir);
    if let Some(age) = age {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        store
            .record(DOMAIN, &run(), &[], now - age, |p, c| diff(p, c, &[], None))
            .await
            .unwrap();
    }
    router(ResultState::new(Arc::new(store), Arc::new(Idle), 300))
}

async fn get(app: Router, path: &str) -> (StatusCode, String) {
    let resp = app
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

/// Every `<img ...>` tag of `html` whose own `src` attribute (not `data-src`) starts with `/badge/`.
fn badge_fetching_imgs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("<img") {
        let tail = &rest[i..];
        let end = tail.find('>').map_or(tail.len(), |e| e + 1);
        let tag = &tail[..end];
        for quote in ['"', '\''] {
            let needle = format!("src={quote}/badge/");
            let mut from = 0;
            while let Some(at) = tag[from..].find(&needle) {
                let at = from + at;
                if tag[..at].ends_with(char::is_whitespace) {
                    out.push(tag.to_string());
                }
                from = at + needle.len();
            }
        }
        rest = &tail[end..];
    }
    out
}

#[tokio::test]
async fn a_page_view_holds_no_img_that_fetches_a_badge() {
    // stale head: the badge route would start a run; fresh head and no entry for completeness
    for (label, age, path) in [
        ("stale", Some(3600), format!("/{DOMAIN}")),
        ("fresh", Some(10), format!("/{DOMAIN}")),
        ("none", None, format!("/{DOMAIN}")),
        ("refusal", Some(3600), format!("/{DOMAIN}?dkim_selectors=,")),
    ] {
        let (status, page) = get(app(age).await, &path).await;
        assert_eq!(
            status,
            if label == "refusal" {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            },
            "{label}"
        );
        let imgs = badge_fetching_imgs(&page);
        assert!(imgs.is_empty(), "{label}: {imgs:?}");
        if age.is_some() && label != "refusal" {
            assert!(
                page.contains(r#"data-src="/badge/example.com.svg""#),
                "{label}: the preview keeps its source in data-src: {page}"
            );
        }
    }
}

#[test]
fn the_scan_tells_src_from_data_src() {
    assert!(badge_fetching_imgs(r#"<img class="b" data-src="/badge/x.svg">"#).is_empty());
    assert_eq!(
        badge_fetching_imgs(r#"<img class="b" src="/badge/x.svg">"#).len(),
        1
    );
}
