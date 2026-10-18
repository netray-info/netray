//! The result page run (spec v2-result-page, Phase 3; C10, C12): `POST /{domain}/run` streams
//! fragments whose evidence links follow the recording: `/r/<id>/<owner>#ev-<block>` once the run
//! is a stored entry, `#ev-<block>` while it streams or when nothing was stored.
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
use netray_model::evidence::{Block, BlockId, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunEvents, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const TTL: u64 = 300;
const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

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

fn run(complete: bool) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: netray_model::evidence::EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete,
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

/// An older entry that differs from `run()` in grade and checks.
fn older_run() -> Run {
    let mut r = run(true);
    r.grade = Grade::A;
    r.checks = vec![RunCheck {
        id: CheckId::parse("tls.chain_trusted").unwrap(),
        status: Status::Pass,
        findings: vec![],
        evidence: vec![],
    }];
    r.blocks = vec![];
    r
}

struct StubRuns {
    store: Arc<HistoryStore>,
    complete: bool,
}

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.trim().to_lowercase())
    }
    fn start<'a>(
        &'a self,
        domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async move {
            let done = run(self.complete);
            let recorded = self
                .store
                .record(domain, &done, &[], now(), |p, c| diff(p, c, &[], None))
                .await
                .unwrap();
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
            events.push(RunEvent::Done(done));
            events.push(RunEvent::Recorded(Some(recorded)));
            let stream: RunEvents = Box::pin(futures::stream::iter(events));
            RunStart::Admitted(stream)
        })
    }
}

struct Fixture {
    app: Router,
    store: Arc<HistoryStore>,
    _dir: tempfile::TempDir,
}

async fn fixture(complete: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    store
        .record(DOMAIN, &older_run(), &[], now() - 1000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let runs = Arc::new(StubRuns {
        store: store.clone(),
        complete,
    });
    let app = router(ResultState::new(store.clone(), runs, TTL));
    Fixture {
        app,
        store,
        _dir: dir,
    }
}

/// (event name, fragment id, html) per SSE event.
async fn post_run(app: &Router) -> (StatusCode, Vec<(String, String, String)>) {
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
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut events = Vec::new();
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
        let (id, html) = match serde_json::from_str::<serde_json::Value>(&data) {
            Ok(j) => (
                j["id"].as_str().unwrap_or_default().to_owned(),
                j["html"].as_str().unwrap_or_default().to_owned(),
            ),
            Err(_) => (String::new(), String::new()),
        };
        events.push((name, id, html));
    }
    (status, events)
}

/// Fragments before the first `result-header` (the section phase) and from it on (the end phase).
fn split_phases(
    events: &[(String, String, String)],
) -> (&[(String, String, String)], &[(String, String, String)]) {
    let at = events
        .iter()
        .position(|(_, id, _)| id == "result-header")
        .unwrap_or_else(|| panic!("no result-header fragment in {events:?}"));
    events.split_at(at)
}

fn html_of<'a>(events: &'a [(String, String, String)], id: &str) -> &'a str {
    events
        .iter()
        .rev()
        .find(|(_, i, _)| i == id)
        .map(|(_, _, h)| h.as_str())
        .unwrap_or_else(|| panic!("no fragment {id} in {events:?}"))
}

#[tokio::test]
async fn c10_c12_evidence_links_follow_the_recording() {
    // (complete run, Changed expected)
    for (complete, changed) in [(true, true), (false, false)] {
        let f = fixture(complete).await;
        let (status, events) = post_run(&f.app).await;
        assert_eq!(status, StatusCode::OK, "complete={complete}");
        let (streaming, end) = split_phases(&events);

        let live: Vec<&str> = streaming
            .iter()
            .filter(|(_, id, _)| id == "issues")
            .map(|(_, _, h)| h.as_str())
            .collect();
        assert!(
            live.iter().any(|h| h.contains(r##"href="#ev-dns.caa""##)),
            "streaming issues, complete={complete}: {live:?}"
        );

        let issues = html_of(end, "issues");
        if changed {
            let id = f.store.latest(DOMAIN).await.unwrap().unwrap().id;
            let want = format!(r##"href="/r/{id}/dns#ev-dns.caa""##);
            assert!(issues.contains(&want), "{want} missing in {issues}");
            for p in ["dns", "tls", "http", "email", "ip"] {
                let html = html_of(end, &format!("evidence-{p}"));
                let want = format!(r#"href="/r/{id}/{p}""#);
                assert!(html.contains(&want), "{want} missing in {html}");
            }

            let get = f
                .app
                .clone()
                .oneshot(
                    Request::get(format!("/{DOMAIN}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let bytes = get.into_body().collect().await.unwrap().to_bytes();
            let page = String::from_utf8_lossy(&bytes);
            assert!(page.contains(&want_issue_link(&id)), "{page}");
        } else {
            assert!(issues.contains(r##"href="#ev-dns.caa""##), "{issues}");
            assert!(!issues.contains("/r/"), "{issues}");
        }
    }
}

fn want_issue_link(id: &str) -> String {
    format!(r##"href="/r/{id}/dns#ev-dns.caa""##)
}
