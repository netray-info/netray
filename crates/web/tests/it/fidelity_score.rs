//! Result fidelity (spec v2-result-fidelity, Phase 1, requirement 5; C5, C14, C15, C16): the grade
//! block shows the score and the grade's descriptor when the run is complete and the stored grade
//! is the one `score_run` gives; otherwise it shows the letter alone.
use std::io::Write;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::history::score_run;
use netray_engine::profile::Profiles;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::grade_legend::descriptor;
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;
use tracing_subscriber::fmt::MakeWriter;

const DOMAIN: &str = "example.com";

/// dns.ns is weighted 3 and warns (half), the other nine dns checks (weights 5,5,5,5,2,2,3,2,3 = 32)
/// pass: 33.5 of 35 earned, 95.71 %, shown as 95 (rounded down: a score never reaches the next
/// grade threshold under a lower letter).
const DNS_PASS: [&str; 9] = [
    "dns.dnssec",
    "dns.caa",
    "dns.ns_lame",
    "dns.cname_apex",
    "dns.https_svcb",
    "dns.ttl",
    "dns.dnskey_algorithm",
    "dns.dnssec_rollover",
    "dns.ns_delegation",
];

fn run(grade: Grade, complete: bool) -> Run {
    let mut checks: Vec<RunCheck> = DNS_PASS
        .iter()
        .map(|id| RunCheck {
            id: CheckId::parse(id).unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        })
        .collect();
    checks.push(RunCheck {
        id: CheckId::parse("dns.ns").unwrap(),
        status: Status::Warn,
        findings: vec![],
        evidence: vec![],
    });
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: if complete {
                SectionKind::Measured
            } else {
                SectionKind::TimedOut
            },
            reason: None,
        }],
        checks,
        blocks: vec![],
    }
}

struct NoRuns;

impl DomainRuns for NoRuns {
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

/// Streams an incomplete run that is not stored.
struct IncompleteRuns;

impl DomainRuns for IncompleteRuns {
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
        let events = vec![
            RunEvent::Done(run(Grade::Incomplete, false)),
            RunEvent::Recorded(Some(Recorded::NotStored { diff: None })),
        ];
        Box::pin(async move { RunStart::Admitted(Box::pin(futures::stream::iter(events))) })
    }
}

async fn store_with(
    dir: &tempfile::TempDir,
    stored: Option<&Run>,
) -> (Arc<HistoryStore>, Option<String>) {
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let mut id = None;
    if let Some(r) = stored {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        match store
            .record(DOMAIN, r, &[], now - 60, |p, c| diff(p, c, &[], None))
            .await
            .unwrap()
        {
            Recorded::First { id: i } => id = Some(i),
            other => panic!("expected First, got {other:?}"),
        }
    }
    (Arc::new(store), id)
}

async fn page(store: Arc<HistoryStore>) -> (StatusCode, String) {
    let app: Router = router(ResultState::new(store, Arc::new(NoRuns), 300));
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Capture {
        self.clone()
    }
}

#[test]
fn c5_every_grade_has_the_v1_legend_descriptor() {
    let rows = [
        (Grade::APlus, "excellent, ahead of most domains"),
        (Grade::A, "strong, minor polish possible"),
        (Grade::B, "ok, weaknesses worth fixing"),
        (Grade::C, "risky, fix before your next audit"),
        (Grade::D, "broken, users will notice"),
        (Grade::F, "critical, fix today"),
    ];
    for (grade, text) in rows {
        assert_eq!(descriptor(grade), text, "{grade:?}");
    }
}

#[tokio::test]
async fn c5_c14_a_stored_complete_entry_shows_score_and_descriptor_under_the_letter() {
    let profile = Profiles::get(3).unwrap();
    let probe = run(Grade::A, true);
    let scored = score_run(&probe, profile);
    assert!(scored.complete);
    assert_eq!(scored.overall_percentage.floor(), 95.0, "{scored:?}");
    let letter = match scored.grade {
        Grade::APlus => "A+",
        Grade::A => "A",
        Grade::B => "B",
        Grade::C => "C",
        Grade::D => "D",
        Grade::F => "F",
        Grade::Incomplete => panic!("complete run graded incomplete"),
    };
    let text = match scored.grade {
        Grade::APlus => "excellent, ahead of most domains",
        Grade::A => "strong, minor polish possible",
        Grade::B => "ok, weaknesses worth fixing",
        Grade::C => "risky, fix before your next audit",
        Grade::D => "broken, users will notice",
        Grade::F => "critical, fix today",
        Grade::Incomplete => unreachable!(),
    };

    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(&dir, Some(&run(scored.grade, true))).await;
    let (status, body) = page(store).await;

    assert_eq!(status, StatusCode::OK);
    let at = |needle: &str| {
        body.find(needle)
            .unwrap_or_else(|| panic!("{needle} missing in {body}"))
    };
    let l = at(&format!(r#"grade__letter">{letter}<"#));
    let s = at(r#"<span class="grade__score">score <strong>95</strong> / 100</span>"#);
    let d = at(&format!(r#"<span class="grade__descriptor">{text}</span>"#));
    assert!(l < s && s < d, "{l} {s} {d}");
}

#[tokio::test]
async fn c15_an_incomplete_live_run_shows_no_score() {
    // contrast: the same page machinery does show a score for a complete stored entry
    let profile = Profiles::get(3).unwrap();
    let grade = score_run(&run(Grade::A, true), profile).grade;
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(&dir, Some(&run(grade, true))).await;
    let (_, stored_page) = page(store).await;
    assert!(
        stored_page.contains(r#"class="grade__score""#),
        "{stored_page}"
    );

    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(&dir, None).await;
    let app: Router = router(ResultState::new(store, Arc::new(IncompleteRuns), 300));
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
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("grade__letter"), "{text}");
    assert!(!text.contains("grade__score"), "{text}");
    assert!(!text.contains("grade__descriptor"), "{text}");
}

#[tokio::test]
async fn c16_a_grade_the_engine_disputes_hides_the_score_and_warns_with_the_entry_id() {
    let profile = Profiles::get(3).unwrap();
    let computed = score_run(&run(Grade::A, true), profile).grade;
    let stored = if computed == Grade::F {
        Grade::A
    } else {
        Grade::F
    };
    let dir = tempfile::tempdir().unwrap();
    let (store, id) = store_with(&dir, Some(&run(stored, true))).await;
    let id = id.unwrap();

    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    let (status, body) = page(store).await;
    drop(guard);

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("grade__letter"), "{body}");
    assert!(!body.contains("grade__score"), "{body}");
    assert!(!body.contains("grade__descriptor"), "{body}");
    let logged = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(
        logged
            .lines()
            .any(|l| l.contains("WARN") && l.contains(&id)),
        "{logged}"
    );
}
