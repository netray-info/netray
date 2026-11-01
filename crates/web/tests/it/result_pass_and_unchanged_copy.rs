//! Pass-row and unchanged-recheck copy (spec pustina-findings R5, R6).
use std::net::IpAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::views::{FIRST_STATE, passed_checks};
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
async fn unchanged_recheck_of_one_entry_shows_no_first_recording_copy() {
    let body = page_after(1).await;
    let lower = body.to_lowercase();
    assert!(
        lower.contains("unchanged since"),
        "no 'unchanged since' in {body}"
    );
    assert!(!lower.contains("first run"), "'first run' in {body}");
    assert!(
        !body.contains("First run recorded."),
        "'First run recorded.' in {body}"
    );
    assert!(
        !body.contains(FIRST_STATE),
        "first-state note {FIRST_STATE:?} in {body}"
    );
}

fn all_pass_run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: [
            Protocol::Dns,
            Protocol::Tls,
            Protocol::Http,
            Protocol::Email,
            Protocol::Ip,
        ]
        .into_iter()
        .map(|protocol| RunSection {
            protocol,
            outcome: SectionKind::Measured,
            reason: None,
        })
        .collect(),
        checks: catalogue()
            .entries()
            .iter()
            .map(|s| RunCheck {
                id: s.id.clone(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

#[test]
fn pass_row_body_is_checks_then_if_it_failed_impact() {
    let cat = catalogue();
    let rows = passed_checks(&all_pass_run(), cat);
    assert_eq!(rows.len(), cat.entries().len());
    let raw: toml::Value = toml::from_str(include_str!("../../../engine/catalogue/checks.toml"))
        .expect("checks.toml parses");
    let tables = raw["check"].as_array().expect("check tables");
    for (row, spec) in rows.iter().zip(cat.entries()) {
        let checks = tables
            .iter()
            .find(|t| t.get("id").and_then(|v| v.as_str()) == Some(row.code.as_str()))
            .and_then(|t| t.get("checks"))
            .and_then(|v| v.as_str())
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| panic!("{}: catalogue has no checks text", row.code));
        let want = format!("If it failed: {}", spec.impact);
        let at_checks = row.body.find(checks);
        let at_impact = row.body.find(&want);
        assert!(
            at_checks.is_some() && at_impact.is_some() && at_checks < at_impact,
            "{}: body {:?} must hold {:?} then {:?}",
            row.code,
            row.body,
            checks,
            want
        );
    }
}
