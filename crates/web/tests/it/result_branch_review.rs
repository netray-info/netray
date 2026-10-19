//! Review findings of v2-result-page, `ecf44678..ecf2aa01` (F1, F2, F4): the header button after a
//! first streamed run, the CSP of a 405 on a domain path, the grade notes of a rescored diff.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::diff::{Diff, GradeChange, diff};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages;
use netray_web::pages::result::{ResultState, router};
use serde_json::Value;
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const BUTTON_VISIT: &str = r#"<button id="recheck-button" class="button button--primary" type="submit" name="trigger" value="visit""#;

fn run(status: Status) -> Run {
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

struct StubRuns {
    store: Option<Arc<HistoryStore>>,
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
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async move {
            let Some(store) = &self.store else {
                return RunStart::NotStarted;
            };
            let done = run(Status::Pass);
            let recorded = store
                .record(DOMAIN, &done, &[], now(), |p, c| diff(p, c, &[], None))
                .await
                .unwrap();
            let events = vec![
                RunEvent::Section(SectionEvent {
                    protocol: Protocol::Tls,
                    outcome: SectionOutcome::Measured {
                        checks: vec![CheckResult {
                            id: CheckId::parse("tls.chain_trusted").unwrap(),
                            status: Status::Pass,
                            findings: vec![],
                            evidence: vec![],
                        }],
                        evidence: vec![],
                    },
                }),
                RunEvent::Done(done),
                RunEvent::Recorded(Some(recorded)),
            ];
            RunStart::Admitted(Box::pin(futures::stream::iter(events)))
        })
    }
}

async fn store() -> (Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (Arc::new(store), dir)
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

async fn get(app: &Router, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    send(app, Request::get(path).body(Body::empty()).unwrap()).await
}

/// (id, html) of every fragment event.
fn fragments(body: &str) -> Vec<(String, String)> {
    body.split("\n\n")
        .filter(|e| !e.trim().is_empty())
        .filter_map(|e| {
            let mut name = "";
            let mut data = String::new();
            for line in e.lines() {
                if let Some(v) = line.strip_prefix("event: ") {
                    name = v;
                } else if let Some(v) = line.strip_prefix("data: ") {
                    data.push_str(v);
                }
            }
            (name == "fragment").then(|| {
                let v: Value = serde_json::from_str(&data).expect(&data);
                (
                    v["id"].as_str().unwrap().to_string(),
                    v["html"].as_str().unwrap().to_string(),
                )
            })
        })
        .collect()
}

/// The `<button id="recheck-button" ...>...</button>` element of `html`.
fn button_of(html: &str) -> &str {
    let start = html
        .find(r#"<button id="recheck-button""#)
        .unwrap_or_else(|| panic!("no recheck-button in {html}"));
    let end = start + html[start..].find("</button>").unwrap() + "</button>".len();
    &html[start..end]
}

#[tokio::test]
async fn f1_the_header_button_turns_into_recheck_after_the_first_streamed_run() {
    let (store, _dir) = store().await;
    let runs = Arc::new(StubRuns {
        store: Some(store.clone()),
    });
    let app = router(ResultState::new(store.clone(), runs, 300));

    let (_, _, before) = get(&app, &format!("/{DOMAIN}")).await;
    assert!(before.contains(BUTTON_VISIT), "{before}");

    let (status, _, body) = send(
        &app,
        Request::post(format!("/{DOMAIN}/run"))
            .header("content-type", "application/x-www-form-urlencoded")
            .header("accept", "text/event-stream")
            .body(Body::from("trigger=visit"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let frags = fragments(&body);
    for (id, html) in &frags {
        assert!(!html.contains(r#"data-island="stream""#), "{id}: {html}");
    }
    let button = frags
        .iter()
        .find(|(id, _)| id == "recheck-button")
        .unwrap_or_else(|| panic!("no recheck-button fragment: {body}"));
    assert!(
        button
            .1
            .trim_start()
            .starts_with(r#"<button id="recheck-button""#),
        "{}",
        button.1
    );
    assert!(button.1.contains(r#"value="recheck""#), "{}", button.1);

    let (_, _, after) = get(&app, &format!("/{DOMAIN}")).await;
    assert_eq!(button.1.trim(), button_of(&after));
}

#[tokio::test]
async fn f2_a_wrong_method_on_a_domain_path_carries_the_v2_pages_csp() {
    let (store, _dir) = store().await;
    let app = router(ResultState::new(
        store.clone(),
        Arc::new(StubRuns { store: None }),
        300,
    ));
    let (gs, gh, _) = get(&pages::router(), "/guide/").await;
    assert_eq!(gs, StatusCode::OK);
    let expected = gh["content-security-policy"].to_str().unwrap().to_string();

    let put = Request::put(format!("/{DOMAIN}"))
        .body(Body::empty())
        .unwrap();
    let cases = [
        ("PUT /shop.example.com", send(&app, put).await),
        (
            "GET /shop.example.com/run",
            get(&app, &format!("/{DOMAIN}/run")).await,
        ),
    ];
    for (name, (status, headers, _)) in cases {
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{name}");
        let csp = headers
            .get("content-security-policy")
            .unwrap_or_else(|| panic!("{name}: missing csp"))
            .to_str()
            .unwrap();
        assert_eq!(csp, expected, "{name}");
        let script_src = csp
            .split(';')
            .find(|d| d.trim_start().starts_with("script-src"))
            .unwrap_or_else(|| panic!("{name}: no script-src in {csp}"));
        assert!(!script_src.contains("'unsafe-inline'"), "{name}: {csp}");
    }
}

#[tokio::test]
async fn f4_a_rescored_grade_line_shows_the_profile_and_domain_effect_notes() {
    let (store, _dir) = store().await;
    store
        .record(DOMAIN, &run(Status::Pass), &[], 1_800_000_000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    store
        .record(DOMAIN, &run(Status::Warn), &[], 1_800_086_400, |_, _| {
            Diff {
                checks: vec![],
                evidence: vec![],
                grade: GradeChange {
                    from: Grade::B,
                    rescored: Some(Grade::C),
                    to: Grade::A,
                },
                engine_changed: false,
            }
        })
        .await
        .unwrap();
    let app = router(ResultState::new(
        store.clone(),
        Arc::new(StubRuns { store: None }),
        300,
    ));

    let (status, _, body) = get(&app, &format!("/{DOMAIN}")).await;

    assert_eq!(status, StatusCode::OK);
    let start = body.find(r#"<section id="diff""#).expect("diff section");
    let section = &body[start
        ..start
            + body[start..]
                .find(r#"id="issues""#)
                .expect("issues after diff")];
    assert!(section.contains("profile effect"), "{section}");
    assert!(section.contains("domain effect"), "{section}");
    assert!(
        section.contains("B \u{2192} C (profile effect) \u{2192} A (domain effect)"),
        "{section}"
    );
}
