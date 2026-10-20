//! The result page run (spec v2-result-page, Phase 3 review decisions): a complete run whose
//! record failed (`Recorded(None)`) ends with its own grade and "not recorded"; a run whose
//! history re-read fails after a stored record ends as the recorded entry, links included.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const UNSTORED_ID: &str = "Zz9yXw8v";
const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

#[derive(Clone, Copy)]
enum Record {
    /// `Recorded(None)`.
    Failed,
    /// `Recorded(Some(Changed { id: UNSTORED_ID, .. }))`, the entry never written to the store.
    ChangedButMissing,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn caa_block() -> Block {
    Block {
        id: BlockId::parse("dns.caa").unwrap(),
        title: "CAA".into(),
        shape: Shape::Text("0 issue \"ca.example.net\"".into()),
    }
}

fn caa_check() -> CheckResult {
    CheckResult {
        id: CheckId::parse("tls.caa_compliant").unwrap(),
        status: Status::Warn,
        findings: vec!["issuer not allowed by CAA".into()],
        evidence: vec![BlockId::parse("dns.caa").unwrap()],
    }
}

/// A complete run graded A.
fn done_run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: PROTOCOLS
            .iter()
            .map(|p| RunSection {
                protocol: *p,
                outcome: SectionKind::Measured,
                reason: None,
            })
            .collect(),
        checks: vec![RunCheck {
            id: CheckId::parse("tls.caa_compliant").unwrap(),
            status: Status::Warn,
            findings: vec!["issuer not allowed by CAA".into()],
            evidence: vec![BlockId::parse("dns.caa").unwrap()],
        }],
        blocks: vec![caa_block()],
    }
}

/// An older stored entry that differs from `done_run()`.
fn older_run() -> Run {
    let mut r = done_run();
    r.grade = Grade::C;
    r.checks = vec![RunCheck {
        id: CheckId::parse("tls.chain_trusted").unwrap(),
        status: Status::Fail,
        findings: vec!["chain not trusted".into()],
        evidence: vec![],
    }];
    r.blocks = vec![];
    r
}

struct StubRuns {
    record: Record,
}

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
        let record = self.record;
        Box::pin(async move {
            let mut events: Vec<RunEvent> = PROTOCOLS
                .iter()
                .map(|p| {
                    let (checks, evidence) = match p {
                        Protocol::Tls => (vec![caa_check()], vec![]),
                        Protocol::Dns => (vec![], vec![caa_block()]),
                        _ => (vec![], vec![]),
                    };
                    RunEvent::Section(SectionEvent {
                        protocol: *p,
                        outcome: SectionOutcome::Measured { checks, evidence },
                    })
                })
                .collect();
            let done = done_run();
            let recorded = match record {
                Record::Failed => None,
                Record::ChangedButMissing => Some(Recorded::Changed {
                    id: UNSTORED_ID.to_string(),
                    diff: diff(&older_run(), &done, &[], None),
                }),
            };
            events.push(RunEvent::Done(done));
            events.push(RunEvent::Recorded(recorded));
            RunStart::Admitted(Box::pin(futures::stream::iter(events)))
        })
    }
}

/// (id, html) of every fragment event, in arrival order.
async fn run(record: Record) -> Vec<(String, String)> {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    store
        .record(DOMAIN, &older_run(), &[], now() - 3600, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let app: Router = router(ResultState::new(
        store.clone(),
        Arc::new(StubRuns { record }),
        300,
    ));
    let resp = app
        .oneshot(
            Request::post(format!("/{DOMAIN}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::from("trigger=visit"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut out = Vec::new();
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
        if name == "fragment" {
            let j: serde_json::Value = serde_json::from_str(&data).unwrap();
            out.push((
                j["id"].as_str().unwrap().to_owned(),
                j["html"].as_str().unwrap().to_owned(),
            ));
        }
    }
    out
}

fn last<'a>(fragments: &'a [(String, String)], id: &str) -> &'a str {
    &fragments
        .iter()
        .rev()
        .find(|(i, _)| i == id)
        .unwrap_or_else(|| panic!("no fragment {id} in {fragments:?}"))
        .1
}

/// The text of `html` with tags removed, trimmed.
fn text_of(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[tokio::test]
async fn a_complete_run_whose_record_failed_ends_with_its_grade_and_not_recorded() {
    let fragments = run(Record::Failed).await;

    let grade = last(&fragments, "result-grade");
    assert!(grade.contains(r#"grade__letter">A<"#), "{grade}");
    assert_eq!(text_of(last(&fragments, "run-line")), "not recorded");
    let diff = last(&fragments, "diff");
    assert!(diff.contains("not recorded"), "{diff}");
    assert!(!diff.contains("this run is incomplete"), "{diff}");
    let share = last(&fragments, "result-share");
    assert!(!share.contains("<a"), "{share}");
    assert!(!share.contains("/r/"), "{share}");
}

#[tokio::test]
async fn a_failed_history_re_read_after_a_stored_record_ends_as_the_recorded_entry() {
    let fragments = run(Record::ChangedButMissing).await;

    let grade = last(&fragments, "result-grade");
    assert!(grade.contains(r#"grade__letter">A<"#), "{grade}");
    let run_line = text_of(last(&fragments, "run-line"));
    assert!(!run_line.contains(UNSTORED_ID), "{run_line}");
    let share = last(&fragments, "result-share");
    assert!(
        share.contains(&format!(r#"href="/r/{UNSTORED_ID}""#)),
        "{share}"
    );
    let issues = last(&fragments, "issues");
    let want = format!(r##"href="/r/{UNSTORED_ID}/dns#ev-dns.caa""##);
    assert!(issues.contains(&want), "{want} missing in {issues}");
    for p in ["dns", "tls", "http", "email", "ip"] {
        let html = last(&fragments, &format!("evidence-{p}"));
        let want = format!(r#"href="/r/{UNSTORED_ID}/{p}""#);
        assert!(html.contains(&want), "{want} missing in {html}");
    }
}
