//! The result page's refused run (spec v2-result-page, requirement 21; C4, C16, C17):
//! `POST /{domain}/run` with an event-stream accept answers 429 with `Retry-After`, one `run-line`
//! fragment holding the refusal sentence and a countdown island, then `end`.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RefusalReason, RunRefusal, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";

struct Refusing(RunRefusal);

impl DomainRuns for Refusing {
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
        let refusal = self.0.clone();
        Box::pin(async move { RunStart::Refused(refusal) })
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

/// An app whose runs are refused with `refusal`; with `entry` the store holds one old entry.
async fn app(refusal: RunRefusal, entry: bool) -> (Router, Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    if entry {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        store
            .record(DOMAIN, &run(), &[], now - 3600, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
    let app = router(ResultState::new(
        store.clone(),
        Arc::new(Refusing(refusal)),
        300,
    ));
    (app, store, dir)
}

struct Answer {
    status: StatusCode,
    retry_after: Option<String>,
    events: Vec<(String, String, String)>,
}

async fn post_sse(app: &Router) -> Answer {
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
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get(header::RETRY_AFTER)
        .map(|v| v.to_str().unwrap().to_owned());
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
    Answer {
        status,
        retry_after,
        events,
    }
}

/// Tags removed, whitespace collapsed.
fn text_of(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn refusal(reason: RefusalReason, retry: u64, ago: Option<u64>, incomplete: bool) -> RunRefusal {
    RunRefusal {
        reason,
        retry_after_s: retry,
        last_run_ago_s: ago,
        last_incomplete: incomplete,
    }
}

#[tokio::test]
async fn c4_c16_c17_a_refused_run_is_429_with_one_run_line_sentence() {
    use RefusalReason::*;
    // (refusal, expected sentence, Retry-After)
    let rows = [
        (
            refusal(Throttle, 20, Some(40), true),
            "a run 40 s ago was incomplete; a new run is possible in 20 s",
            "20",
        ),
        (
            refusal(Throttle, 20, Some(40), false),
            "checked 40 s ago; a new run is possible in 20 s",
            "20",
        ),
        (
            refusal(Budget, 30, None, false),
            "too many checks from your address; try again in 30 s",
            "30",
        ),
        (
            refusal(IpLimit, 30, None, false),
            "too many checks from your address; try again in 30 s",
            "30",
        ),
        (
            refusal(GlobalLimit, 30, None, false),
            "netray is busy; try again in 30 s",
            "30",
        ),
        (
            refusal(Slots, 30, None, false),
            "netray is busy; try again in 30 s",
            "30",
        ),
    ];
    for (r, sentence, retry) in rows {
        let label = format!("{r:?}");
        // With a stored entry, the refusal must not replace the result header.
        let (app, _store, _dir) = app(r, true).await;

        let a = post_sse(&app).await;

        assert_eq!(a.status, StatusCode::TOO_MANY_REQUESTS, "{label}");
        assert_eq!(a.retry_after.as_deref(), Some(retry), "{label}");
        let names: Vec<&str> = a.events.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(names, ["fragment", "end"], "{label}");
        assert_eq!(a.events[0].1, "run-line", "{label}");
        let html = &a.events[0].2;
        assert_eq!(text_of(html), sentence, "{label}: {html}");
        assert!(
            html.contains(r#"data-island="countdown""#),
            "{label}: {html}"
        );
        assert!(
            html.contains(&format!(r#"data-retry-after="{retry}""#)),
            "{label}: {html}"
        );
    }
}
