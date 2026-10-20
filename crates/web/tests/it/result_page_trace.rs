//! The spectrum's trace interactions, server half (spec v2-result-page, Phase 5; requirements 16
//! and 17, C1): the gutter, the sentence line, the phone stepper, the per-line sentences and one
//! hit target per catalogue line.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::views::history_sentence;
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const T: i64 = 1_800_000_000;
const ORDER: [(Protocol, &str); 5] = [
    (Protocol::Dns, "dns.ns"),
    (Protocol::Tls, "tls.chain_trusted"),
    (Protocol::Http, "http.hsts"),
    (Protocol::Email, "email.spf"),
    (Protocol::Ip, "ip.reputation"),
];
/// Oldest first.
const STATUSES: [Status; 3] = [Status::Pass, Status::Warn, Status::Fail];

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_lowercase())
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

fn run(status: Status) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: ORDER
            .iter()
            .map(|(p, _)| RunSection {
                protocol: *p,
                outcome: SectionKind::Measured,
                reason: None,
            })
            .collect(),
        checks: ORDER
            .iter()
            .map(|(_, id)| RunCheck {
                id: CheckId::parse(id).unwrap(),
                status,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

async fn page() -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for (i, status) in STATUSES.iter().enumerate() {
        store
            .record(DOMAIN, &run(*status), &[], T + i as i64 * 100, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
    let app: Router = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600));
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn between<'a>(html: &'a str, from: &str, to: &str) -> &'a str {
    let start = html
        .find(from)
        .unwrap_or_else(|| panic!("no {from} in {html}"));
    let rest = &html[start..];
    &rest[..rest.find(to).unwrap_or(rest.len())]
}

#[tokio::test]
async fn c1_spectrum_holds_gutter_sentence_stepper_sentences_and_targets() {
    let html = page().await;
    let spectrum = between(&html, "<figure id=\"spectrum\"", "</figure>");

    assert!(spectrum.contains("id=\"spectrum-gutter\""), "{spectrum}");
    assert!(spectrum.contains("id=\"spectrum-sentence\""), "{spectrum}");

    let stepper = between(spectrum, "id=\"spectrum-stepper\"", "<div id=\"trace\">");
    assert_eq!(
        stepper.matches("<button type=\"button\"").count(),
        2,
        "{stepper}"
    );
    assert!(stepper.contains("data-step=\"-1\""), "{stepper}");
    assert!(stepper.contains("data-step=\"1\""), "{stepper}");

    let list = between(spectrum, "<ul id=\"spectrum-sentences\" hidden>", "</ul>");
    let want = catalogue().entries().len();
    assert_eq!(list.matches("<li").count(), want, "{list}");
    assert_eq!(list.matches("data-code=\"").count(), want, "{list}");
    assert_eq!(list.matches("data-sentence=\"").count(), want, "{list}");
    let sentence = history_sentence(&[
        (T, Status::Pass),
        (T + 100, Status::Warn),
        (T + 200, Status::Fail),
    ]);
    assert!(
        list.contains(&format!("data-sentence=\"{sentence}\"")),
        "want {sentence}: {list}"
    );

    for spec in catalogue().entries() {
        let target = format!("data-target=\"{}\"", spec.id);
        assert_eq!(spectrum.matches(&target).count(), 1, "{target}: {spectrum}");
    }
}
