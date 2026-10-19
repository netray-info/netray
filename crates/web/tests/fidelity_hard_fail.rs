//! The hard-fail banner (spec v2-result-fidelity, requirement 11; C11, C22, C23, C24).
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const TTL: u64 = 300;

struct StubRuns {
    starts: AtomicUsize,
}

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        let d = domain.trim().to_lowercase();
        let d = d.strip_suffix('.').unwrap_or(&d).to_string();
        if d.is_empty() || !d.contains('.') {
            return Err(DomainRefusal {
                code: "DOMAIN_INVALID",
                message: format!("invalid domain: {domain}"),
            });
        }
        Ok(d)
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { RunStart::NotStarted })
    }
}

fn run_with(grade: Grade, sections: Vec<RunSection>, checks: Vec<RunCheck>) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections,
        checks,
        blocks: vec![],
    }
}

fn section(protocol: Protocol, outcome: SectionKind, reason: Option<&str>) -> RunSection {
    RunSection {
        protocol,
        outcome,
        reason: reason.map(String::from),
    }
}

fn check(id: &str, status: Status) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    }
}

async fn render(run: &Run) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    match store
        .record(DOMAIN, run, &[], now - 60, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { .. } => {}
        other => panic!("expected First, got {other:?}"),
    }
    let runs = Arc::new(StubRuns {
        starts: AtomicUsize::new(0),
    });
    let app: Router = router(ResultState::new(store, runs, TTL));
    let resp = app
        .oneshot(Request::get(format!("/{DOMAIN}")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn pos(body: &str, needle: &str) -> usize {
    body.find(needle)
        .unwrap_or_else(|| panic!("{needle} missing in {body}"))
}

#[tokio::test]
async fn c22_failed_hostname_match_names_its_title_and_the_grade_is_f() {
    let run = run_with(
        Grade::F,
        vec![section(Protocol::Tls, SectionKind::Measured, None)],
        vec![
            check("tls.chain_trusted", Status::Pass),
            check("tls.hostname_match", Status::Fail),
        ],
    );

    let body = render(&run).await;

    let header = pos(&body, r#"id="result-header""#);
    let banner = pos(&body, r#"<section id="hard-fail" class="hard-fail">"#);
    let spectrum = pos(&body, r#"id="spectrum""#);
    assert!(header < banner && banner < spectrum, "order in {body}");
    let end = body[banner..].find("</section>").unwrap();
    let text = &body[banner..banner + end];
    assert!(
        text.contains(
            "Hard fail: Hostname match. A failed hard-fail check sets the grade to F whatever the score."
        ),
        "{text}"
    );
    assert!(!text.contains("hard-fail__without"), "{text}");
    assert!(body.contains(">F<"), "grade letter F missing in {body}");
}

#[tokio::test]
async fn c23_left_out_section_is_named_with_its_reason() {
    let reason = "no MX records";
    let run = run_with(
        Grade::A,
        vec![
            section(Protocol::Tls, SectionKind::Measured, None),
            section(Protocol::Email, SectionKind::NotApplicable, Some(reason)),
        ],
        vec![check("tls.chain_trusted", Status::Pass)],
    );

    let body = render(&run).await;

    let banner = pos(&body, r#"<section id="hard-fail" class="hard-fail">"#);
    let text = &body[banner..banner + body[banner..].find("</section>").unwrap()];
    assert!(
        text.contains(
            r#"<p class="hard-fail__without">Score computed without: Email (no MX records)</p>"#
        ),
        "{text}"
    );
    assert!(!text.contains("Hard fail:"), "{text}");
}

#[tokio::test]
async fn c24_neither_hard_fail_nor_left_out_section_renders_no_banner() {
    let run = run_with(
        Grade::A,
        vec![section(Protocol::Tls, SectionKind::Measured, None)],
        vec![check("tls.chain_trusted", Status::Pass)],
    );

    let body = render(&run).await;

    assert!(!body.contains(r#"id="hard-fail""#), "{body}");
}
