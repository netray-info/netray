//! Result fidelity Phase 5, review findings: the evidence nav streams with the end fragments
//! so the current tab follows the finished run, and the "recorded with this run" meta shows
//! only over a stored entry.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const META: &str = r#"<p id="evidence-meta" class="evidence-meta">recorded with this run</p>"#;
const META_HIDDEN: &str = r#"<p id="evidence-meta" class="evidence-meta" hidden></p>"#;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn run_of(tls: Status) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
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
        checks: vec![
            RunCheck {
                id: CheckId::parse("dns.caa").unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            },
            RunCheck {
                id: CheckId::parse("tls.dane_valid").unwrap(),
                status: tls,
                findings: vec![],
                evidence: vec![],
            },
        ],
        blocks: vec![],
    }
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
        let section = SectionEvent {
            protocol: Protocol::Dns,
            outcome: SectionOutcome::Measured {
                checks: vec![CheckResult {
                    id: CheckId::parse("dns.caa").unwrap(),
                    status: Status::Pass,
                    findings: vec![],
                    evidence: vec![],
                }],
                evidence: vec![],
            },
        };
        let events = vec![
            RunEvent::Section(section),
            RunEvent::Done(self.done.clone()),
            RunEvent::Recorded(self.recorded.clone()),
        ];
        Box::pin(async move { RunStart::Admitted(Box::pin(futures::stream::iter(events))) })
    }
}

async fn store_of(runs: &[Run]) -> Arc<HistoryStore> {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for (i, run) in runs.iter().enumerate() {
        store
            .record(DOMAIN, run, &[], now() - 10_000 + i as i64 * 60, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
    std::mem::forget(dir);
    Arc::new(store)
}

fn app(store: Arc<HistoryStore>, done: Run, recorded: Option<Recorded>) -> Router {
    router(ResultState::new(
        store,
        Arc::new(StubRuns { done, recorded }),
        300,
    ))
}

async fn page(app: &Router) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// (id, html) of every `fragment` event of a visit run, in arrival order.
async fn run(app: &Router) -> Vec<(String, String)> {
    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/{DOMAIN}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::from("trigger=recheck"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
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
        .unwrap_or_else(|| panic!("no fragment {id}"))
        .1
}

/// The `<nav id="evidence-nav" …>` element of `body`.
fn nav_of(body: &str) -> &str {
    let at = body.find(r#"<nav id="evidence-nav""#).expect("nav missing");
    &body[at..at + body[at..].find("</nav>").unwrap() + "</nav>".len()]
}

fn meta_of(body: &str) -> &str {
    let at = body.find(r#"<p id="evidence-meta""#).expect("meta missing");
    &body[at..at + body[at..].find("</p>").unwrap() + "</p>".len()]
}

#[tokio::test]
async fn the_end_fragments_send_the_nav_after_the_five_panels_and_equal_the_page() {
    let older = run_of(Status::Pass);
    let done = run_of(Status::Warn);
    let store = store_of(std::slice::from_ref(&older)).await;
    let recorded = store
        .record(DOMAIN, &done, &[], now(), |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let app = app(store, done, Some(recorded));

    let fragments = run(&app).await;
    let body = page(&app).await;

    let ids: Vec<&str> = fragments.iter().map(|(i, _)| i.as_str()).collect();
    let nav_at = ids.iter().rposition(|i| *i == "evidence-nav").unwrap();
    for p in ["dns", "tls", "http", "email", "ip"] {
        let at = ids
            .iter()
            .rposition(|i| *i == format!("evidence-{p}"))
            .unwrap();
        assert!(at < nav_at, "{ids:?}");
    }
    assert_eq!(last(&fragments, "evidence-nav"), nav_of(&body));
    assert!(
        last(&fragments, "evidence-nav").contains(r#"data-tab="tls" aria-current="true""#),
        "{}",
        last(&fragments, "evidence-nav")
    );
    assert_eq!(last(&fragments, "evidence-meta"), META);
    assert_eq!(last(&fragments, "evidence-meta"), meta_of(&body));
}

#[tokio::test]
async fn the_live_fragments_send_no_nav() {
    let store = store_of(&[run_of(Status::Pass)]).await;
    let app = app(store.clone(), run_of(Status::Warn), None);

    let fragments = run(&app).await;

    let navs = fragments
        .iter()
        .filter(|(i, _)| i == "evidence-nav")
        .count();
    assert_eq!(navs, 1, "only the end set carries the nav");
}

#[tokio::test]
async fn the_page_shows_the_meta_over_a_stored_entry_only() {
    let never = store_of(&[]).await;
    let stored = store_of(&[run_of(Status::Pass)]).await;

    let empty_page = page(&app(never, run_of(Status::Pass), None)).await;
    let stored_page = page(&app(stored, run_of(Status::Pass), None)).await;

    assert_eq!(meta_of(&empty_page), META_HIDDEN);
    assert!(!empty_page.contains("recorded with this run"));
    assert_eq!(meta_of(&stored_page), META);
}

#[tokio::test]
async fn a_run_start_hides_the_meta_and_an_unrecorded_run_leaves_it_hidden() {
    let store = store_of(&[run_of(Status::Pass)]).await;
    let app = app(store, run_of(Status::Warn), None);

    let fragments = run(&app).await;

    let metas: Vec<&str> = fragments
        .iter()
        .filter(|(i, _)| i == "evidence-meta")
        .map(|(_, h)| h.as_str())
        .collect();
    assert!(metas.len() >= 2, "{metas:?}");
    assert!(metas.iter().all(|m| *m == META_HIDDEN), "{metas:?}");
}
