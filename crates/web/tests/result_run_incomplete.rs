//! The result page (spec v2-result-page, Phase 3; C5 requirement 22, C11): `POST /{domain}/run`
//! streams an incomplete run: the timed-out section is unmeasured, nothing is stored, the end
//! fragments carry no share link and the evidence is the live section partial.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";

/// (protocol, check id, block id) of the sections that measured.
const MEASURED: [(Protocol, &str, &str); 4] = [
    (Protocol::Dns, "dns.ns", "dns.records"),
    (Protocol::Tls, "tls.chain_trusted", "tls.chain"),
    (Protocol::Http, "http.hsts", "http.headers"),
    (Protocol::Ip, "ip.reputation", "ip.addresses"),
];

struct StubRuns {
    done_grade: Grade,
    tls_status: Status,
}

fn block(id: &str) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: id.into(),
        shape: Shape::Fields(vec![("k".into(), Value::Text("v".into()))]),
    }
}

impl StubRuns {
    fn status(&self, protocol: Protocol) -> Status {
        if protocol == Protocol::Tls {
            self.tls_status
        } else {
            Status::Pass
        }
    }

    fn done(&self) -> Run {
        let mut sections = Vec::new();
        for p in [
            Protocol::Dns,
            Protocol::Tls,
            Protocol::Http,
            Protocol::Email,
            Protocol::Ip,
        ] {
            sections.push(RunSection {
                protocol: p,
                outcome: if p == Protocol::Email {
                    SectionKind::TimedOut
                } else {
                    SectionKind::Measured
                },
                reason: None,
            });
        }
        Run {
            profile_version: 3,
            engine_version: "0.26.0".into(),
            evidence_schema_version: netray_model::evidence::EVIDENCE_SCHEMA_VERSION,
            grade: self.done_grade,
            complete: false,
            sections,
            checks: MEASURED
                .iter()
                .map(|(p, c, b)| RunCheck {
                    id: CheckId::parse(c).unwrap(),
                    status: self.status(*p),
                    findings: vec![],
                    evidence: vec![BlockId::parse(b).unwrap()],
                })
                .collect(),
            blocks: MEASURED.iter().map(|(_, _, b)| block(b)).collect(),
        }
    }
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
        let mut events = Vec::new();
        for p in [Protocol::Dns, Protocol::Tls, Protocol::Http] {
            events.push(self.measured(p));
        }
        events.push(RunEvent::Section(SectionEvent {
            protocol: Protocol::Email,
            outcome: SectionOutcome::TimedOut,
        }));
        events.push(self.measured(Protocol::Ip));
        events.push(RunEvent::Done(self.done()));
        events.push(RunEvent::Recorded(Some(Recorded::NotStored { diff: None })));
        Box::pin(async move { RunStart::Admitted(Box::pin(futures::stream::iter(events))) })
    }
}

impl StubRuns {
    fn measured(&self, p: Protocol) -> RunEvent {
        let (_, c, b) = MEASURED.iter().find(|m| m.0 == p).unwrap();
        RunEvent::Section(SectionEvent {
            protocol: p,
            outcome: SectionOutcome::Measured {
                checks: vec![CheckResult {
                    id: CheckId::parse(c).unwrap(),
                    status: self.status(p),
                    findings: vec![],
                    evidence: vec![BlockId::parse(b).unwrap()],
                }],
                evidence: vec![block(b)],
            },
        })
    }
}

/// (event name, id, html) per event.
async fn stream(done_grade: Grade, tls_status: Status) -> Vec<(String, String, String)> {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let runs = Arc::new(StubRuns {
        done_grade,
        tls_status,
    });
    let app: Router = router(ResultState::new(Arc::new(store), runs, 300));
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
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        events.push((
            name,
            j["id"].as_str().unwrap_or_default().to_owned(),
            j["html"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    events
}

fn nth<'a>(events: &'a [(String, String, String)], id: &str, n: usize) -> &'a str {
    events
        .iter()
        .filter(|e| e.0 == "fragment" && e.1 == id)
        .nth(n)
        .unwrap_or_else(|| panic!("fragment {id} #{n} missing in {events:?}"))
        .2
        .as_str()
}

fn last<'a>(events: &'a [(String, String, String)], id: &str) -> &'a str {
    events
        .iter()
        .rev()
        .find(|e| e.0 == "fragment" && e.1 == id)
        .unwrap_or_else(|| panic!("fragment {id} missing in {events:?}"))
        .2
        .as_str()
}

fn assert_live_partial(html: &str, protocol: &str) {
    assert!(html.contains(r#"class="evidence-section""#), "{html}");
    assert!(
        html.contains(&format!(r#"data-protocol="{protocol}""#)),
        "{html}"
    );
    assert!(!html.contains("/r/"), "{html}");
}

#[tokio::test]
async fn c5_c11_an_incomplete_run_streams_unmeasured_and_is_not_recorded() {
    // (Done grade, tls.chain_trusted status, expected end grade letter)
    let rows = [
        (Grade::Incomplete, Status::Pass, "incomplete"),
        (Grade::F, Status::Fail, "F"),
    ];
    for (grade, tls, letter) in rows {
        let events = stream(grade, tls).await;
        assert_eq!(events.last().unwrap().0, "end", "{letter}");

        // after the email section (the fourth): segment, band, live evidence
        let progress = nth(&events, "progress", 4);
        let email = &progress[progress
            .find("<span class=\"progress-ray__label\">email")
            .unwrap()
            - 60..];
        assert!(email.contains(r#"data-state="incomplete""#), "{progress}");
        let spectrum = nth(&events, "spectrum", 4);
        let band = &spectrum[spectrum.find(r#"id="band-email""#).expect("band-email")..];
        let run = &band[band.find(r#"data-run="0""#).expect("data-run 0")..];
        let tag_end = run.find('>').unwrap();
        assert!(run[..tag_end].contains("data-unmeasured"), "{run}");
        assert_live_partial(nth(&events, "evidence-email", 0), "email");

        // end fragments
        let grade_html = last(&events, "result-grade");
        assert!(
            grade_html.contains(&format!(r#"grade__letter">{letter}<"#)),
            "{letter}: {grade_html}"
        );
        if letter == "incomplete" {
            let run_line = last(&events, "run-line");
            assert!(run_line.contains("incomplete, not recorded"), "{run_line}");
            let share = last(&events, "result-share");
            assert!(!share.contains("<a"), "{share}");
            assert!(!share.contains("/r/"), "{share}");
            assert!(
                last(&events, "diff").contains("not recorded: this run is incomplete"),
                "{events:?}"
            );
            for p in ["dns", "tls", "http", "email", "ip"] {
                assert_live_partial(last(&events, &format!("evidence-{p}")), p);
            }
        }
    }
}
