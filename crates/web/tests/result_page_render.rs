//! The result page (spec v2-result-page, Phase 2; C3, C4, C5, C11, C12): `GET /{domain}` renders
//! the latest stored entry without starting a run or touching the store.
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

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

struct Fixture {
    app: Router,
    store: Arc<HistoryStore>,
    runs: Arc<StubRuns>,
    _dir: tempfile::TempDir,
}

/// A store holding one entry for `DOMAIN` recorded `age_s` seconds ago, or empty.
async fn fixture(age_s: Option<i64>) -> (Fixture, Option<String>) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let mut id = None;
    if let Some(age) = age_s {
        match store
            .record(DOMAIN, &run(), &[], now() - age, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap()
        {
            Recorded::First { id: i } => id = Some(i),
            other => panic!("expected First, got {other:?}"),
        }
    }
    let runs = Arc::new(StubRuns {
        starts: AtomicUsize::new(0),
    });
    let app = router(ResultState::new(store.clone(), runs.clone(), TTL));
    (
        Fixture {
            app,
            store,
            runs,
            _dir: dir,
        },
        id,
    )
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

/// Unix seconds as "YYYY-MM-DD HH:MM" (proleptic Gregorian, days-to-civil).
fn utc_minute(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

fn pos(body: &str, needle: &str) -> usize {
    body.find(needle)
        .unwrap_or_else(|| panic!("{needle} missing in {body}"))
}

fn assert_section_order(body: &str) {
    let marks = [
        r#"id="result-header""#,
        r#"id="spectrum""#,
        r#"id="diff""#,
        r#"id="issues""#,
        r#"id="evidence""#,
        r#"<details id="passed""#,
    ];
    let at: Vec<usize> = marks.iter().map(|m| pos(body, m)).collect();
    assert!(
        at.windows(2).all(|w| w[0] < w[1]),
        "order {at:?} of {marks:?}"
    );
}

#[tokio::test]
async fn c3_c11_get_renders_the_latest_entry_without_a_run_or_a_write() {
    let (f, id) = fixture(Some(60)).await;
    let id = id.unwrap();
    let before = f.store.latest(DOMAIN).await.unwrap().unwrap();

    let (status, body) = get(&f.app, &format!("/{DOMAIN}")).await;

    assert_eq!(status, StatusCode::OK);
    assert_section_order(&body);
    let passed = &body[pos(&body, r#"<details id="passed""#)..];
    let tag = &passed[..passed.find('>').unwrap()];
    assert!(!tag.contains("open"), "{tag}");
    assert!(body.contains(&format!(r#"href="/r/{id}""#)), "{body}");
    assert_eq!(f.runs.starts.load(Ordering::SeqCst), 0);
    let after = f.store.latest(DOMAIN).await.unwrap().unwrap();
    assert_eq!(after.last_seen_at, before.last_seen_at);
    assert_eq!(f.store.entries(DOMAIN).await.unwrap().len(), 1);
}

#[tokio::test]
async fn c3_without_an_entry_the_page_says_not_checked_yet() {
    let (f, _) = fixture(None).await;

    let (status, body) = get(&f.app, &format!("/{DOMAIN}")).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("not checked yet"), "{body}");
    assert!(body.contains(r#"name="trigger" value="visit""#), "{body}");
    assert_eq!(f.runs.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn c4_run_line_names_the_entry_time_and_id() {
    let (f, id) = fixture(Some(60)).await;
    let id = id.unwrap();
    let created = f.store.latest(DOMAIN).await.unwrap().unwrap().created_at;

    let (_, body) = get(&f.app, &format!("/{DOMAIN}")).await;

    let want = format!("{} UTC", utc_minute(created));
    assert!(body.contains(&want), "{want} missing in {body}");
    assert!(body.contains(&format!(r#"href="/r/{id}""#)), "{body}");
}

#[tokio::test]
async fn c5_share_link_only_for_a_stored_entry() {
    let (f, id) = fixture(Some(60)).await;
    let (_, body) = get(&f.app, &format!("/{DOMAIN}")).await;
    assert!(body.contains(r#"id="share""#), "{body}");
    assert!(
        body.contains(&format!(r#"href="/r/{}""#, id.unwrap())),
        "{body}"
    );

    let (empty, _) = fixture(None).await;
    let (_, body) = get(&empty.app, &format!("/{DOMAIN}")).await;
    assert!(!body.contains(r#"id="share""#), "{body}");
}

#[tokio::test]
async fn c4_c12_data_state_and_stream_autostart_follow_the_age() {
    // (age in seconds, data-state, autostart expected, "not checked yet" expected)
    let rows = [
        (Some(600), "stale", true, false),
        (Some(60), "fresh", false, false),
        (None, "none", true, true),
    ];
    for (age, state, autostart, unchecked) in rows {
        let (f, _) = fixture(age).await;
        let (status, body) = get(&f.app, &format!("/{DOMAIN}")).await;

        assert_eq!(status, StatusCode::OK, "{state}");
        assert!(
            body.contains(&format!(r#"data-state="{state}""#)),
            "{state}: {body}"
        );
        assert!(body.contains(r#"data-island="stream""#), "{state}: {body}");
        assert!(
            body.contains(&format!(r#"data-stream-url="/{DOMAIN}/run""#)),
            "{state}: {body}"
        );
        assert_eq!(
            body.contains(r#"data-stream-autostart="visit""#),
            autostart,
            "{state}: {body}"
        );
        assert_eq!(body.contains("not checked yet"), unchecked, "{state}");
        assert_eq!(f.runs.starts.load(Ordering::SeqCst), 0, "{state}");
    }
}
