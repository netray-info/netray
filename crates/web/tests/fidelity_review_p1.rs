//! Result fidelity Phase 1, review findings: the run title and diff heading of a record the
//! history re-read did not return, the grade-mismatch warning on an incomplete score, the
//! hard-fail sentence under a disputed letter, and the banner of runs that were not stored or
//! not recorded.
use std::io::Write;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

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
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;
use tracing_subscriber::fmt::MakeWriter;

const DOMAIN: &str = "shop.example.com";
const MISSING_ID: &str = "Zz9yXw8v";
const HARD_FAIL: &str =
    "Hard fail: Hostname match. A failed hard-fail check sets the grade to F whatever the score.";

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

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn check(id: &str, status: Status) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    }
}

fn section(protocol: Protocol, outcome: SectionKind, reason: Option<&str>) -> RunSection {
    RunSection {
        protocol,
        outcome,
        reason: reason.map(String::from),
    }
}

fn run_with(grade: Grade, complete: bool, sections: Vec<RunSection>, checks: Vec<RunCheck>) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete,
        sections,
        checks,
        blocks: vec![],
    }
}

/// A complete run graded A with one warning.
fn plain_run() -> Run {
    run_with(
        Grade::A,
        true,
        vec![section(Protocol::Tls, SectionKind::Measured, None)],
        vec![check("tls.caa_compliant", Status::Warn)],
    )
}

/// A run that differs from `plain_run()`.
fn older_run() -> Run {
    run_with(
        Grade::C,
        true,
        vec![section(Protocol::Tls, SectionKind::Measured, None)],
        vec![check("tls.chain_trusted", Status::Fail)],
    )
}

/// A failed hostname match (a hard-fail check) and a left-out email section.
fn hard_fail_run(grade: Grade, complete: bool) -> Run {
    run_with(
        grade,
        complete,
        vec![
            section(Protocol::Tls, SectionKind::Measured, None),
            section(
                Protocol::Email,
                SectionKind::NotApplicable,
                Some("no MX records"),
            ),
        ],
        vec![
            check("tls.chain_trusted", Status::Pass),
            check("tls.hostname_match", Status::Fail),
        ],
    )
}

/// A complete run whose dns.ns is unmeasured: its score is incomplete.
fn unmeasured_run(grade: Grade) -> Run {
    let mut checks: Vec<RunCheck> = DNS_PASS.iter().map(|id| check(id, Status::Pass)).collect();
    checks.push(check("dns.ns", Status::Unmeasured));
    run_with(
        grade,
        true,
        vec![section(Protocol::Dns, SectionKind::Measured, None)],
        checks,
    )
}

struct StubRuns {
    done: Run,
    recorded: Option<Recorded>,
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
        let events = vec![
            RunEvent::Done(self.done.clone()),
            RunEvent::Recorded(self.recorded.clone()),
        ];
        Box::pin(async move { RunStart::Admitted(Box::pin(futures::stream::iter(events))) })
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

async fn open_store(dir: &tempfile::TempDir, stored: &[&Run]) -> (Arc<HistoryStore>, Vec<String>) {
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let mut ids = Vec::new();
    for (i, run) in stored.iter().enumerate() {
        let at = now() - 3600 + i as i64;
        let recorded = store
            .record(DOMAIN, run, &[], at, |p, c| diff(p, c, &[], None))
            .await
            .unwrap();
        match recorded {
            Recorded::First { id } | Recorded::Changed { id, .. } => ids.push(id),
            other => panic!("unexpected {other:?}"),
        }
    }
    (Arc::new(store), ids)
}

/// (id, html) of every fragment of the streamed run.
async fn stream(
    store: Arc<HistoryStore>,
    done: Run,
    recorded: Option<Recorded>,
) -> Vec<(String, String)> {
    let app: Router = router(ResultState::new(
        store,
        Arc::new(StubRuns { done, recorded }),
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

async fn page(store: Arc<HistoryStore>) -> String {
    let app: Router = router(ResultState::new(store, Arc::new(NoRuns), 300));
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

fn last<'a>(fragments: &'a [(String, String)], id: &str) -> &'a str {
    &fragments
        .iter()
        .rev()
        .find(|(i, _)| i == id)
        .unwrap_or_else(|| panic!("no fragment {id} in {fragments:?}"))
        .1
}

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

/// The process-wide capture of every log line: a thread-local subscriber races other tests'
/// callsite registrations, so the subscriber is global and each test looks for its own entry id.
fn capture() -> Capture {
    static CAPTURE: OnceLock<Capture> = OnceLock::new();
    CAPTURE
        .get_or_init(|| {
            let capture = Capture::default();
            let subscriber = tracing_subscriber::fmt()
                .with_writer(capture.clone())
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .finish();
            tracing::subscriber::set_global_default(subscriber).unwrap();
            capture
        })
        .clone()
}

/// The grade `score_run` gives `run` under its profile, and a letter that differs from it.
fn disputed(run: &Run) -> (Grade, Grade) {
    let computed = score_run(run, Profiles::get(run.profile_version).unwrap()).grade;
    let stored = if computed == Grade::A {
        Grade::F
    } else {
        Grade::A
    };
    (computed, stored)
}

// Finding 1: a record the history re-read did not return

#[tokio::test]
async fn a_changed_record_missing_from_the_re_read_has_no_ordinal_and_says_since_the_previous_run()
{
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = open_store(&dir, &[&older_run()]).await;
    let done = plain_run();
    let recorded = Recorded::Changed {
        id: MISSING_ID.to_string(),
        diff: diff(&older_run(), &done, &[], None),
    };

    let fragments = stream(store, done, Some(recorded)).await;

    let title = text_of(last(&fragments, "run-line"));
    assert!(title.contains("UTC"), "{title}");
    assert!(!title.contains("Run "), "{title}");
    assert!(!title.contains("first run"), "{title}");
    let diff = last(&fragments, "diff");
    assert!(diff.contains("<h2>Since the previous run</h2>"), "{diff}");
    assert!(!diff.contains("First run recorded."), "{diff}");
}

#[tokio::test]
async fn an_identical_record_missing_from_the_re_read_has_no_ordinal_and_says_since_the_previous_run()
 {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = open_store(&dir, &[&plain_run()]).await;
    let recorded = Recorded::Identical {
        id: MISSING_ID.to_string(),
    };

    let fragments = stream(store, plain_run(), Some(recorded)).await;

    let title = text_of(last(&fragments, "run-line"));
    assert!(!title.contains("Run "), "{title}");
    assert!(!title.contains("first run"), "{title}");
    let diff = last(&fragments, "diff");
    assert!(diff.contains("<h2>Since the previous run</h2>"), "{diff}");
    assert!(!diff.contains("First run recorded."), "{diff}");
}

#[tokio::test]
async fn a_first_record_missing_from_the_re_read_keeps_the_first_run_wording() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = open_store(&dir, &[]).await;
    let recorded = Recorded::First {
        id: MISSING_ID.to_string(),
    };

    let fragments = stream(store, plain_run(), Some(recorded)).await;

    let title = text_of(last(&fragments, "run-line"));
    assert!(title.contains("Run 1"), "{title}");
    assert!(title.contains("first run"), "{title}");
    let diff = last(&fragments, "diff");
    assert!(diff.contains("<h2>First run recorded.</h2>"), "{diff}");
}

// Finding 2: a disputed grade is logged whether or not the score is complete

#[tokio::test]
async fn a_disputed_grade_on_an_incomplete_score_warns_with_the_entry_id_and_hides_the_score() {
    let probe = unmeasured_run(Grade::A);
    let profile = Profiles::get(3).unwrap();
    let scored = score_run(&probe, profile);
    assert!(!scored.complete, "{scored:?}");
    let (_, stored) = disputed(&probe);
    let dir = tempfile::tempdir().unwrap();
    let (store, ids) = open_store(&dir, &[&unmeasured_run(stored)]).await;

    let capture = capture();
    let body = page(store).await;

    assert!(!body.contains("grade__score"), "{body}");
    let logged = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(
        logged
            .lines()
            .any(|l| l.contains("WARN") && l.contains(&ids[0])),
        "{logged}"
    );
}

// Finding 3: the hard-fail sentence names F, so it needs the letter F

#[tokio::test]
async fn the_hard_fail_sentence_stays_out_under_a_disputed_letter_but_the_left_out_line_stays() {
    let probe = hard_fail_run(Grade::A, true);
    let (computed, stored) = disputed(&probe);
    assert_eq!(computed, Grade::F);
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = open_store(&dir, &[&hard_fail_run(stored, true)]).await;

    let body = page(store).await;

    assert!(!body.contains("Hard fail:"), "{body}");
    assert!(
        body.contains(
            r#"<p class="hard-fail__without">Score computed without: Email (no MX records)</p>"#
        ),
        "{body}"
    );
}

// Finding 4: runs that were not stored or not recorded get the banner

#[tokio::test]
async fn a_run_that_was_not_stored_shows_the_banner() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = open_store(&dir, &[]).await;

    let fragments = stream(
        store,
        hard_fail_run(Grade::F, false),
        Some(Recorded::NotStored { diff: None }),
    )
    .await;

    let banner = last(&fragments, "hard-fail-slot");
    assert!(banner.contains(HARD_FAIL), "{banner}");
    assert!(
        banner.contains("Score computed without: Email (no MX records)"),
        "{banner}"
    );
}

#[tokio::test]
async fn a_complete_run_that_was_not_recorded_shows_the_banner() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = open_store(&dir, &[]).await;

    let fragments = stream(store, hard_fail_run(Grade::F, true), None).await;

    let banner = last(&fragments, "hard-fail-slot");
    assert!(banner.contains(HARD_FAIL), "{banner}");
    assert!(
        banner.contains("Score computed without: Email (no MX records)"),
        "{banner}"
    );
}

#[tokio::test]
async fn an_unrecorded_run_under_a_disputed_letter_omits_the_hard_fail_sentence() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = open_store(&dir, &[]).await;

    let fragments = stream(store, hard_fail_run(Grade::A, true), None).await;

    let banner = last(&fragments, "hard-fail-slot");
    assert!(!banner.contains("Hard fail:"), "{banner}");
    assert!(
        banner.contains("Score computed without: Email (no MX records)"),
        "{banner}"
    );
}
