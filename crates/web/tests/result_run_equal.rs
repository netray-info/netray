//! The result page run (spec v2-result-page, Phase 3; C3/R20, C7, C9): the end fragments of
//! `POST /{domain}/run` equal the elements of `GET /{domain}` rendered after the run.
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
const NA_REASON: &str = "no TLS service answers on this name";

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn check(id: &str, status: Status, findings: &[&str]) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: findings.iter().map(|s| s.to_string()).collect(),
        evidence: vec![],
    }
}

fn section(protocol: Protocol, outcome: SectionKind, reason: Option<&str>) -> RunSection {
    RunSection {
        protocol,
        outcome,
        reason: reason.map(str::to_string),
    }
}

fn older() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![
            section(Protocol::Dns, SectionKind::Measured, None),
            section(Protocol::Tls, SectionKind::Measured, None),
        ],
        checks: vec![
            check("dns.ns", Status::Pass, &[]),
            check("tls.dane_valid", Status::Pass, &[]),
        ],
        blocks: vec![],
    }
}

/// The finished run: dns fails a check, tls is not applicable with dane_valid not_applicable.
fn finished() -> Run {
    Run {
        grade: Grade::C,
        sections: vec![
            section(Protocol::Dns, SectionKind::Measured, None),
            section(Protocol::Tls, SectionKind::NotApplicable, Some(NA_REASON)),
        ],
        checks: vec![
            check("dns.ns", Status::Fail, &["only one name server answers"]),
            check("tls.dane_valid", Status::NotApplicable, &[]),
        ],
        ..older()
    }
}

fn live_sections() -> Vec<SectionEvent> {
    let result = |id: &str, status, findings: &[&str]| {
        let c = check(id, status, findings);
        CheckResult {
            id: c.id,
            status: c.status,
            findings: c.findings,
            evidence: c.evidence,
        }
    };
    vec![
        SectionEvent {
            protocol: Protocol::Dns,
            outcome: SectionOutcome::Measured {
                checks: vec![result(
                    "dns.ns",
                    Status::Fail,
                    &["only one name server answers"],
                )],
                evidence: vec![],
            },
        },
        SectionEvent {
            protocol: Protocol::Tls,
            outcome: SectionOutcome::Measured {
                checks: vec![result("tls.dane_valid", Status::NotTested, &[])],
                evidence: vec![],
            },
        },
    ]
}

struct StubRuns {
    store: Arc<HistoryStore>,
}

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_lowercase())
    }
    fn start<'a>(
        &'a self,
        domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async move {
            let run = finished();
            let recorded = self
                .store
                .record(domain, &run, &[], now(), |p, c| diff(p, c, &[], None))
                .await
                .unwrap();
            assert!(matches!(recorded, Recorded::Changed { .. }), "{recorded:?}");
            let mut events: Vec<RunEvent> =
                live_sections().into_iter().map(RunEvent::Section).collect();
            events.push(RunEvent::Done(run));
            events.push(RunEvent::Recorded(Some(recorded)));
            RunStart::Admitted(Box::pin(futures::stream::iter(events)))
        })
    }
}

async fn fixture() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    store
        .record(DOMAIN, &older(), &[], now() - 3600, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let runs = Arc::new(StubRuns {
        store: store.clone(),
    });
    (router(ResultState::new(store, runs, 300)), dir)
}

/// (id, html) of every `fragment` event, in arrival order.
async fn run(app: &Router) -> Vec<(String, String)> {
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
    assert_eq!(resp.status(), 200, "POST /{{domain}}/run answers 200 SSE");
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
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The element carrying `id="<id>"`: from its opening `<` through the matching close tag.
fn element(body: &str, id: &str) -> String {
    let at = body
        .find(&format!(r#"id="{id}""#))
        .unwrap_or_else(|| panic!("id {id} missing in {body}"));
    let start = body[..at].rfind('<').unwrap();
    let tag: String = body[start + 1..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut depth = 0usize;
    let mut i = start;
    loop {
        let rest = &body[i..];
        let next_open = rest.find(&open).filter(|&o| {
            rest[o + open.len()..]
                .chars()
                .next()
                .is_some_and(|c| c == '>' || c.is_whitespace() || c == '/')
        });
        let next_close = rest.find(&close).expect("unclosed element");
        match next_open {
            Some(o) if o < next_close => {
                depth += 1;
                i += o + open.len();
            }
            _ => {
                depth -= 1;
                i += next_close + close.len();
                if depth == 0 {
                    return body[start..i].to_string();
                }
            }
        }
    }
}

/// The end fragments: the last fragment of each id.
fn last<'a>(fragments: &'a [(String, String)], id: &str) -> &'a str {
    &fragments
        .iter()
        .rev()
        .find(|(i, _)| i == id)
        .unwrap_or_else(|| panic!("no fragment {id}"))
        .1
}

const END_IDS: [&str; 14] = [
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
];

#[tokio::test]
async fn c3_c7_end_fragments_equal_the_elements_of_the_page_after_a_changed_run() {
    let (app, _dir) = fixture().await;

    let fragments = run(&app).await;
    let body = page(&app).await;

    // The page renders the fresh entry as "cached"; the run that just made it was not.
    let cached = r#"&#32;<span class="run-line__sep" aria-hidden="true">·</span>&#32;<span class="run-line__part">cached</span>"#;
    for id in END_IDS {
        assert_eq!(
            last(&fragments, id),
            element(&body, id).replace(cached, ""),
            "end fragment {id} differs from the page"
        );
    }
}

#[tokio::test]
async fn c9_end_fragments_show_a_check_not_tested_live_as_not_applicable() {
    let (app, _dir) = fixture().await;

    let fragments = run(&app).await;

    let issues = last(&fragments, "issues");
    assert!(!issues.contains("tls.dane_valid"), "{issues}");
    assert!(issues.contains("dns.ns"), "{issues}");
    let passed = last(&fragments, "passed");
    assert!(passed.contains("tls.dane_valid"), "{passed}");
    assert!(passed.contains(NA_REASON), "{passed}");
    let spectrum = last(&fragments, "spectrum");
    assert!(
        spectrum.contains(r#"data-code="tls.dane_valid""#),
        "{spectrum}"
    );
    assert!(!spectrum.contains("data-refreshing"), "{spectrum}");
}
