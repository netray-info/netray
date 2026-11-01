//! A re-check recorded `Identical` says "unchanged since <created_at>" and is not a first run
//! (spec pustina-findings C5, C12, C13).
use std::net::IpAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "example.com";
/// 2023-11-14 22:13 UTC.
const CREATED: i64 = 1_700_000_000;
/// 2023-11-16 22:13 UTC.
const RECHECKED: i64 = CREATED + 2 * 86_400;

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

fn run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

async fn page_after(rechecks: usize) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let first = store
        .record(DOMAIN, &run(), &[], CREATED, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    assert!(matches!(first, Recorded::First { .. }), "{first:?}");
    for _ in 0..rechecks {
        let again = store
            .record(DOMAIN, &run(), &[], RECHECKED, |p, c| diff(p, c, &[], None))
            .await
            .unwrap();
        assert!(matches!(again, Recorded::Identical { .. }), "{again:?}");
    }
    let app = router(ResultState::new(store, Arc::new(StubRuns), 300));
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned()
}

#[tokio::test]
async fn first_run_text_only_for_a_genuine_first_recording() {
    // (case, re-checks recorded Identical, first-run text expected)
    let cases = [
        ("C13 first recording", 0usize, true),
        ("C5/C12 identical re-check", 1, false),
    ];
    for (name, rechecks, first_run) in cases {
        let body = page_after(rechecks).await;
        let lower = body.to_lowercase();
        assert_eq!(
            body.contains("First run recorded."),
            first_run,
            "{name}: First run recorded. in {body}"
        );
        assert_eq!(
            lower.contains("first run"),
            first_run,
            "{name}: first run in {body}"
        );
        if !first_run {
            assert!(
                lower.contains("unchanged since 2023-11-14"),
                "{name}: no 'unchanged since 2023-11-14' in {body}"
            );
            assert!(
                body.contains("2023-11-16"),
                "{name}: last checked date missing in {body}"
            );
        }
    }
}
