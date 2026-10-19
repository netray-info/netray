//! Result fidelity, Phase 4 (spec v2-result-fidelity, requirement 32): an open item's since text
//! comes from the domain's stored entries, computed by one pure function.
use std::net::IpAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::Status::{self, Fail, Pass, Warn};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::views::since_text;
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

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

fn some(v: &[Status]) -> Vec<Option<Status>> {
    v.iter().copied().map(Some).collect()
}

#[test]
fn since_text_follows_the_check_history() {
    let twelve = [Warn; 12];
    let cases: Vec<(&str, Vec<Option<Status>>, usize, Option<&str>)> = vec![
        (
            "lit up at the third run",
            some(&[Pass, Pass, Warn, Warn]),
            1,
            Some("lit up in run 3"),
        ),
        ("warn in all 12", some(&twelve), 1, Some("in all 12 runs")),
        (
            "down from fail",
            some(&[Fail, Fail, Warn]),
            1,
            Some("down from fail since run 3"),
        ),
        (
            "up from warn",
            some(&[Warn, Fail]),
            1,
            Some("up from warn since run 2"),
        ),
        ("first run", some(&[Warn]), 1, Some("first run")),
        (
            "last span counts",
            some(&[Pass, Warn, Pass, Warn]),
            1,
            Some("lit up in run 4"),
        ),
        (
            "offset run numbers",
            some(&[Fail, Warn, Warn]),
            10,
            Some("down from fail since run 11"),
        ),
        ("latest is pass", some(&[Warn, Pass]), 1, None),
        (
            "absent counts as not open",
            vec![None, Some(Warn)],
            1,
            Some("lit up in run 2"),
        ),
    ];
    for (name, statuses, first, want) in cases {
        assert_eq!(
            since_text(&statuses, first).as_deref(),
            want,
            "case: {name}"
        );
    }
}

fn run(caa: Status, other: Status) -> Run {
    let check = |id: &str, status| RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    };
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
        // dns.dnssec_valid alternates so that no entry equals its predecessor.
        checks: vec![check("dns.caa", caa), check("dns.dnssec_valid", other)],
        blocks: vec![],
    }
}

#[tokio::test]
async fn the_page_reads_since_from_the_stored_entries() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let caa = [Pass, Pass, Warn, Warn];
    let other = [Pass, Warn, Pass, Warn];
    for i in 0..4 {
        store
            .record(
                "example.com",
                &run(caa[i], other[i]),
                &[],
                now - 3600 + i as i64 * 60,
                |p, c| diff(p, c, &[], None),
            )
            .await
            .unwrap();
    }
    let app = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 300));
    let resp = app
        .oneshot(Request::get("/example.com").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body =
        String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned();
    let row = body
        .split(r#"<article class="check-row""#)
        .skip(1)
        .find(|r| r.contains("dns.caa"))
        .unwrap_or_else(|| panic!("no dns.caa row in {body}"));
    assert!(
        row.contains(r#"<span class="check-row__since">lit up in run 3</span>"#),
        "{row}"
    );
}
