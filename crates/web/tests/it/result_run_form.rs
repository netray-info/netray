//! The result page's recheck form (spec v2-result-page, Phase 3; C1, C19): a form POST without the
//! SSE accept runs to the end and redirects; a refusal answers the full page with the refusal line.
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use futures::{StreamExt, stream};
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunSection, SectionKind};
use netray_model::{Grade, Protocol};
use netray_store::HistoryStore;
use netray_web::api::{
    DomainRefusal, DomainRuns, RefusalReason, RunEvent, RunEvents, RunRefusal, RunStart, RunTrigger,
};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";

struct StubRuns {
    refusal: Option<RunRefusal>,
    consumed: Arc<AtomicBool>,
    triggers: Mutex<Vec<RunTrigger>>,
}

fn done_run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::NotApplicable,
            reason: Some("none".into()),
        }],
        checks: vec![],
        blocks: vec![],
    }
}

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.trim().to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        self.triggers.lock().unwrap().push(trigger);
        let start = match &self.refusal {
            Some(r) => RunStart::Refused(r.clone()),
            None => {
                let flag = self.consumed.clone();
                let events = vec![
                    Some(RunEvent::Section(SectionEvent {
                        protocol: Protocol::Dns,
                        outcome: SectionOutcome::NotApplicable {
                            reason: "none".into(),
                        },
                    })),
                    Some(RunEvent::Section(SectionEvent {
                        protocol: Protocol::Tls,
                        outcome: SectionOutcome::TimedOut,
                    })),
                    Some(RunEvent::Done(done_run())),
                    Some(RunEvent::Recorded(None)),
                ];
                let s: RunEvents = Box::pin(
                    stream::iter(events)
                        .chain(stream::once(async move {
                            flag.store(true, Ordering::SeqCst);
                            None
                        }))
                        .filter_map(|e| async move { e }),
                );
                RunStart::Admitted(s)
            }
        };
        Box::pin(async move { start })
    }
}

async fn app(refusal: Option<RunRefusal>) -> (Router, Arc<StubRuns>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let runs = Arc::new(StubRuns {
        refusal,
        consumed: Arc::new(AtomicBool::new(false)),
        triggers: Mutex::new(vec![]),
    });
    let app = router(ResultState::new(Arc::new(store), runs.clone(), 300));
    (app, runs, dir)
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    (
        parts.status,
        parts.headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn form_post() -> Request<Body> {
    Request::post(format!("/{DOMAIN}/run"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("trigger=recheck"))
        .unwrap()
}

fn strip_tags(html: &str) -> String {
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

fn element<'a>(body: &'a str, id: &str) -> &'a str {
    let at = body
        .find(&format!(r#"id="{id}""#))
        .unwrap_or_else(|| panic!("id {id} missing in {body}"));
    &body[at..]
}

#[tokio::test]
async fn c1_the_page_form_posts_the_recheck_trigger_to_the_run_route() {
    let (app, _runs, dir) = app(None).await;
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store
        .record(DOMAIN, &done_run(), &[], 1_700_000_000, |p, c| {
            netray_model::diff::diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let (status, _, body) = send(
        &app,
        Request::get(format!("/{DOMAIN}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<form"), "{body}");
    assert!(body.contains(r#"action="/shop.example.com/run""#), "{body}");
    assert!(body.contains(r#"method="post""#), "{body}");
    assert!(body.contains(r#"name="trigger" value="recheck""#), "{body}");
}

#[tokio::test]
async fn c1_c19_a_form_post_runs_to_the_end_then_redirects_to_the_page() {
    let (app, runs, _dir) = app(None).await;

    let (status, headers, body) = send(&app, form_post()).await;

    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(headers.get(header::LOCATION).unwrap(), "/shop.example.com");
    assert!(
        runs.consumed.load(Ordering::SeqCst),
        "the stream was not read to the end before the response"
    );
    assert_eq!(*runs.triggers.lock().unwrap(), vec![RunTrigger::Recheck]);
}

#[tokio::test]
async fn c19_a_refused_form_post_answers_429_with_the_full_page_and_the_refusal_line() {
    let refusal = RunRefusal {
        reason: RefusalReason::Budget,
        retry_after_s: 30,
        last_run_ago_s: None,
        last_incomplete: false,
    };
    let (app, _runs, _dir) = app(Some(refusal)).await;

    let (status, headers, body) = send(&app, form_post()).await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(headers.get(header::RETRY_AFTER).unwrap(), "30");
    assert!(body.contains(r#"id="result""#), "{body}");
    assert!(body.contains(r#"id="result-header""#), "{body}");
    let line = element(&body, "run-line");
    let line = &line[line.find('>').unwrap() + 1..];
    let line = &line[..line.find("</div>").unwrap()];
    assert!(
        strip_tags(line).contains("too many checks from your address; try again in 30 s"),
        "{line}"
    );
}
