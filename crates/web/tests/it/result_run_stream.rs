//! `POST /{domain}/run` on the result page (spec v2-result-page, Phase 3; C2, C6, C8, C13, C14,
//! C18): the fragment sequence of a streamed run, the fresh-head short cut and the prefetch refusal.
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
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
use serde_json::Value;
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const TTL: u64 = 300;
const ORDER: [(Protocol, &str, &str); 5] = [
    (Protocol::Dns, "dns", "dns.ns"),
    (Protocol::Tls, "tls", "tls.chain_trusted"),
    (Protocol::Http, "http", "http.hsts"),
    (Protocol::Email, "email", "email.spf"),
    (Protocol::Ip, "ip", "ip.reputation"),
];

#[derive(Clone, Copy)]
enum Mode {
    /// Sections in this order of `ORDER` indexes, then Done and Recorded.
    Admit(&'static [usize]),
    NotStarted,
}

struct Call {
    domain: String,
    trigger: RunTrigger,
    prefetch: bool,
    client: IpAddr,
}

struct StubRuns {
    store: Arc<HistoryStore>,
    mode: Mode,
    calls: Mutex<Vec<Call>>,
}

fn complete_run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: ORDER
            .iter()
            .map(|(p, _, _)| RunSection {
                protocol: *p,
                outcome: SectionKind::Measured,
                reason: None,
            })
            .collect(),
        checks: ORDER
            .iter()
            .map(|(_, _, id)| RunCheck {
                id: CheckId::parse(id).unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

async fn record(store: &HistoryStore, run: &Run, at: i64) -> Recorded {
    store
        .record(DOMAIN, run, &[], at, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
}

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        if domain.contains('.') {
            Ok(domain.to_string())
        } else {
            Err(DomainRefusal {
                code: "DOMAIN_INVALID",
                message: format!("invalid domain: {domain}"),
            })
        }
    }

    fn start<'a>(
        &'a self,
        domain: &'a str,
        client: IpAddr,
        trigger: RunTrigger,
        prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        self.calls.lock().unwrap().push(Call {
            domain: domain.to_string(),
            trigger,
            prefetch,
            client,
        });
        Box::pin(async move {
            let Mode::Admit(order) = self.mode else {
                return RunStart::NotStarted;
            };
            let mut events: Vec<RunEvent> = order
                .iter()
                .map(|i| {
                    let (protocol, _, id) = ORDER[*i];
                    RunEvent::Section(SectionEvent {
                        protocol,
                        outcome: SectionOutcome::Measured {
                            checks: vec![CheckResult {
                                id: CheckId::parse(id).unwrap(),
                                status: Status::Pass,
                                findings: vec![],
                                evidence: vec![],
                            }],
                            evidence: vec![],
                        },
                    })
                })
                .collect();
            let run = complete_run();
            let recorded = record(&self.store, &run, now()).await;
            events.push(RunEvent::Done(run));
            events.push(RunEvent::Recorded(Some(recorded)));
            RunStart::Admitted(Box::pin(futures::stream::iter(events)))
        })
    }
}

struct Fixture {
    app: Router,
    runs: Arc<StubRuns>,
    store: Arc<HistoryStore>,
    _dir: tempfile::TempDir,
}

async fn fixture(mode: Mode, head_age_s: Option<i64>) -> (Fixture, Option<String>) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let mut id = None;
    if let Some(age) = head_age_s {
        match record(&store, &complete_run(), now() - age).await {
            Recorded::First { id: i } => id = Some(i),
            other => panic!("expected First, got {other:?}"),
        }
    }
    let runs = Arc::new(StubRuns {
        store: store.clone(),
        mode,
        calls: Mutex::new(vec![]),
    });
    let app = router(ResultState::new(store.clone(), runs.clone(), TTL));
    (
        Fixture {
            app,
            runs,
            store,
            _dir: dir,
        },
        id,
    )
}

struct Reply {
    status: StatusCode,
    cache_control: Option<String>,
    body: String,
}

async fn post(app: &Router, trigger: &str, headers: &[(&str, &str)]) -> Reply {
    let mut req = Request::post(format!("/{DOMAIN}/run"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "text/event-stream");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::from(format!("trigger={trigger}"))).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let cache_control = resp
        .headers()
        .get("cache-control")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        cache_control,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

/// (event name, data) of every SSE event.
fn events(body: &str) -> Vec<(String, Value)> {
    body.split("\n\n")
        .filter(|e| !e.trim().is_empty())
        .map(|e| {
            let mut name = String::new();
            let mut data = String::new();
            for line in e.lines() {
                if let Some(v) = line.strip_prefix("event: ") {
                    name = v.to_string();
                } else if let Some(v) = line.strip_prefix("data: ") {
                    data.push_str(v);
                }
            }
            (name, serde_json::from_str(&data).expect(&data))
        })
        .collect()
}

/// (id, html) of the fragments, asserting that the last event is `end`.
fn fragments(body: &str) -> Vec<(String, String)> {
    let all = events(body);
    assert_eq!(all.last().map(|e| e.0.as_str()), Some("end"), "{body}");
    all[..all.len() - 1]
        .iter()
        .map(|(name, data)| {
            assert_eq!(name, "fragment", "{body}");
            (
                data["id"].as_str().unwrap().to_string(),
                data["html"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn ids(frags: &[(String, String)]) -> Vec<&str> {
    frags.iter().map(|(id, _)| id.as_str()).collect()
}

/// The opening tag of the element with `id` inside `html`.
fn tag_of<'a>(html: &'a str, id: &str) -> &'a str {
    let at = html
        .find(&format!(r#"id="{id}""#))
        .unwrap_or_else(|| panic!("id {id} missing in {html}"));
    let start = html[..at].rfind('<').unwrap();
    &html[start..start + html[start..].find('>').unwrap()]
}

const CANON: &[usize] = &[0, 1, 2, 3, 4];

fn expected_ids(order: &[usize]) -> Vec<String> {
    let mut want: Vec<String> = ["run-line", "progress", "spectrum"]
        .map(String::from)
        .to_vec();
    for i in order {
        let p = ORDER[*i].1;
        for f in ["progress", "spectrum", "issues"] {
            want.push(f.to_string());
        }
        want.push(format!("evidence-{p}"));
    }
    for f in [
        "result-grade",
        "run-line",
        "progress",
        "result-share",
        "hard-fail-slot",
        "spectrum",
        "diff",
        "issues",
    ] {
        want.push(f.to_string());
    }
    for (_, p, _) in ORDER {
        want.push(format!("evidence-{p}"));
    }
    want.push("passed".to_string());
    // A first visit's recorded run turns the check button into re-check (branch review F1).
    want.push("recheck-button".to_string());
    want
}

#[tokio::test]
async fn c2_recheck_reaches_start_and_the_first_fragments_show_a_running_run() {
    let (f, _) = fixture(Mode::Admit(CANON), None).await;

    let reply = post(&f.app, "recheck", &[]).await;

    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    {
        let calls = f.runs.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].domain, DOMAIN);
        assert_eq!(calls[0].trigger, RunTrigger::Recheck);
        assert!(!calls[0].prefetch);
        assert_eq!(calls[0].client, "127.0.0.1".parse::<IpAddr>().unwrap());
    }
    let frags = fragments(&reply.body);
    assert_eq!(&ids(&frags)[..3], ["run-line", "progress", "spectrum"]);
    assert!(frags[0].1.contains("running"), "{}", frags[0].1);
    assert_eq!(frags[1].1.matches(r#"data-state="pending""#).count(), 5);
    assert!(
        !frags[1].1.contains(r#"data-state="done""#),
        "{}",
        frags[1].1
    );
    assert!(frags[2].1.contains("data-refreshing"), "{}", frags[2].1);
}

#[tokio::test]
async fn c6_c8_fragments_arrive_in_section_order_and_carry_their_own_id() {
    let (f, _) = fixture(Mode::Admit(CANON), None).await;

    let reply = post(&f.app, "visit", &[]).await;

    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let frags = fragments(&reply.body);
    assert_eq!(ids(&frags), expected_ids(CANON));

    // The form lives in #result-header and is never re-sent.
    for (id, html) in &frags {
        assert_ne!(id, "result-header");
        assert!(!html.contains(r#"data-island="stream""#), "{id}: {html}");
    }
    for (id, html) in &frags {
        let trimmed = html.trim_start();
        assert!(trimmed.starts_with('<'), "{id}: {html}");
        assert!(!trimmed.starts_with("<g"), "{id}: {html}");
        assert!(
            tag_of(trimmed, id).len() == trimmed[..trimmed.find('>').unwrap()].len(),
            "{id} is not the root element: {html}"
        );
    }
    // The spectrum fragments are figures.
    for (id, html) in frags.iter().filter(|(id, _)| id == "spectrum") {
        assert!(
            html.trim_start().starts_with(r#"<figure id="spectrum""#),
            "{id}: {html}"
        );
    }
    // After the dns section: band-dns is live, band-tls is not.
    let after_dns = &frags[4].1;
    assert_eq!(frags[4].0, "spectrum");
    assert!(
        tag_of(after_dns, "band-dns").contains("data-live"),
        "{after_dns}"
    );
    assert!(
        !tag_of(after_dns, "band-tls").contains("data-live"),
        "{after_dns}"
    );
    assert!(after_dns.contains("data-refreshing"), "{after_dns}");
    // The end spectrum is no longer refreshing.
    let end_spectrum = frags.iter().rfind(|(id, _)| id == "spectrum").unwrap();
    assert!(
        !end_spectrum.1.contains("data-refreshing"),
        "{}",
        end_spectrum.1
    );
}

#[tokio::test]
async fn c13_a_joined_run_passes_its_finished_sections_through_in_arrival_order() {
    // ip and email finished before this client joined; dns, tls, http follow.
    const JOINED: &[usize] = &[4, 3, 0, 1, 2];
    let (f, _) = fixture(Mode::Admit(JOINED), None).await;

    let reply = post(&f.app, "visit", &[]).await;

    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let frags = fragments(&reply.body);
    assert_eq!(ids(&frags), expected_ids(JOINED));
}

#[tokio::test]
async fn c14_a_fresh_head_answers_from_the_store_on_visit_and_runs_on_recheck() {
    let (f, id) = fixture(Mode::Admit(CANON), Some(60)).await;
    let id = id.unwrap();

    let visit = post(&f.app, "visit", &[]).await;

    assert_eq!(visit.status, StatusCode::OK, "{}", visit.body);
    assert!(f.runs.calls.lock().unwrap().is_empty());
    let frags = fragments(&visit.body);
    assert_eq!(
        ids(&frags),
        [
            "result-grade",
            "run-line",
            "progress",
            "result-share",
            "hard-fail-slot",
            "spectrum",
            "diff",
            "issues",
            "evidence-dns",
            "evidence-tls",
            "evidence-http",
            "evidence-email",
            "evidence-ip",
            "passed",
            "recheck-button"
        ]
    );
    let run_line = &frags.iter().find(|(i, _)| i == "run-line").unwrap().1;
    assert!(!run_line.contains(&id), "{run_line}");
    assert!(run_line.contains("cached"), "{run_line}");
    assert_eq!(f.store.entries(DOMAIN).await.unwrap().len(), 1);

    let recheck = post(&f.app, "recheck", &[]).await;

    assert_eq!(recheck.status, StatusCode::OK, "{}", recheck.body);
    let calls = f.runs.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].trigger, RunTrigger::Recheck);
}

#[tokio::test]
async fn c18_a_prefetch_that_would_start_a_run_answers_503_no_store() {
    let (f, _) = fixture(Mode::NotStarted, None).await;

    let reply = post(&f.app, "visit", &[("sec-purpose", "prefetch")]).await;

    assert_eq!(
        reply.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        reply.body
    );
    assert_eq!(reply.cache_control.as_deref(), Some("no-store"));
    let calls = f.runs.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].prefetch);
}
