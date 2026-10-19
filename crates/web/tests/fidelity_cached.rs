//! Spec v2-result-fidelity, Phase 1 (R10; C10, C19): "cached" is a run title part when the
//! result page renders a stored entry inside the cache window, and the run stream's answer from
//! that entry carries it too.
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
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
const TTL: u64 = 300;

struct StubRuns {
    starts: AtomicUsize,
}

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
        self.starts.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { RunStart::NotStarted })
    }
}

fn run(grade: Grade, status: Status) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status,
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

/// A store with `ages` (seconds ago, oldest first) as entries of differing runs.
async fn fixture(ages: &[i64]) -> (Router, Arc<StubRuns>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    for (i, age) in ages.iter().enumerate() {
        let r = if i % 2 == 0 {
            run(Grade::A, Status::Pass)
        } else {
            run(Grade::C, Status::Fail)
        };
        store
            .record(DOMAIN, &r, &[], now() - age, |p, c| diff(p, c, &[], None))
            .await
            .unwrap();
    }
    let runs = Arc::new(StubRuns {
        starts: AtomicUsize::new(0),
    });
    let app = router(ResultState::new(store, runs.clone(), TTL));
    (app, runs, dir)
}

/// The parts of the `<div id="run-line">` element.
fn run_line_parts(html: &str) -> Vec<String> {
    let at = html
        .find(r#"<div id="run-line">"#)
        .unwrap_or_else(|| panic!("run-line missing in {html}"));
    let rest = &html[at..];
    let rest = &rest[..rest.find("</div>").unwrap()];
    rest.split(r#"<span class="run-line__part">"#)
        .skip(1)
        .map(|s| s[..s.find("</span>").unwrap()].to_string())
        .collect()
}

async fn get(app: &Router) -> String {
    let resp = app
        .clone()
        .oneshot(Request::get(format!("/{DOMAIN}")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

async fn stream_run_line(app: &Router) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/{DOMAIN}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::from("trigger=visit"))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    for raw in text.split("\n\n") {
        let data: String = raw
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect();
        if let Ok(j) = serde_json::from_str::<serde_json::Value>(&data)
            && j["id"] == "run-line"
        {
            return j["html"].as_str().unwrap().to_string();
        }
    }
    panic!("no run-line fragment in {text}");
}

#[tokio::test]
async fn cached_is_a_run_title_part_only_inside_the_cache_window() {
    // (entry ages oldest first, data-state, last part, "cached" expected)
    let rows: [(&[i64], &str, bool); 3] = [
        (&[200, 60], "fresh", true),
        (&[900, 600], "stale", false),
        (&[60], "fresh", true),
    ];
    for (ages, state, cached) in rows {
        let (app, runs, _dir) = fixture(ages).await;
        let body = get(&app).await;
        assert!(body.contains(&format!(r#"data-state="{state}""#)), "{body}");
        let parts = run_line_parts(&body);
        assert_eq!(parts.iter().any(|p| p == "cached"), cached, "{ages:?}: {parts:?}");
        if cached && ages.len() > 1 {
            assert_eq!(parts.last().unwrap(), "cached", "{parts:?}");
        }
        if cached && ages.len() == 1 {
            let n = parts.len();
            assert_eq!(parts[n - 2..], ["cached", "first run"], "{parts:?}");
        }
        assert_eq!(runs.starts.load(Ordering::SeqCst), 0, "{ages:?}");
    }
}

#[tokio::test]
async fn run_stream_answered_from_a_fresh_entry_marks_the_run_line_cached() {
    let (app, runs, _dir) = fixture(&[200, 60]).await;

    let html = stream_run_line(&app).await;

    let parts = run_line_parts(&html);
    assert_eq!(parts.last().unwrap(), "cached", "{parts:?}");
    assert_eq!(runs.starts.load(Ordering::SeqCst), 0);
}
