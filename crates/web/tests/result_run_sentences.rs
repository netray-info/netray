//! Phase 5 review (spec v2-result-page): the end fragments' line sentences include an unstored
//! run as the newest point, and the run stepper renders only with two or more runs.
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
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const CHECK: &str = "tls.chain_trusted";

fn run(status: Status, complete: bool) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: if complete {
            Grade::B
        } else {
            Grade::Incomplete
        },
        complete,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse(CHECK).unwrap(),
            status,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

struct StubRuns;

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
        let events = vec![
            RunEvent::Section(SectionEvent {
                protocol: Protocol::Tls,
                outcome: SectionOutcome::Measured {
                    checks: vec![CheckResult {
                        id: CheckId::parse(CHECK).unwrap(),
                        status: Status::Fail,
                        findings: vec![],
                        evidence: vec![],
                    }],
                    evidence: vec![],
                },
            }),
            RunEvent::Done(run(Status::Fail, false)),
            RunEvent::Recorded(Some(Recorded::NotStored { diff: None })),
        ];
        Box::pin(async move { RunStart::Admitted(Box::pin(futures::stream::iter(events))) })
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// MM-DD of a unix time, UTC (days-to-civil).
fn month_day(t: i64) -> String {
    let z = t.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let _ = era;
    format!("{m:02}-{d:02}")
}

async fn store(entries: usize) -> (tempfile::TempDir, HistoryStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for i in 0..entries {
        let at = now() - 86_400 * 5 * (entries - i) as i64;
        let r = run(if i == 0 { Status::Pass } else { Status::Warn }, true);
        store
            .record(DOMAIN, &r, &[], at, |p, c| diff(p, c, &[], None))
            .await
            .unwrap();
    }
    (dir, store)
}

fn app(store: HistoryStore) -> Router {
    router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 300))
}

async fn end_spectrum(entries: usize) -> String {
    let (_dir, st) = store(entries).await;
    let resp = app(st)
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
    let mut last = String::new();
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
        if name == "fragment" && j["id"] == "spectrum" {
            last = j["html"].as_str().unwrap_or_default().to_owned();
        }
    }
    assert!(!last.is_empty(), "no spectrum fragment");
    last
}

fn sentence(spectrum: &str) -> String {
    let list = &spectrum[spectrum
        .find("<ul id=\"spectrum-sentences\" hidden>")
        .expect("sentence list")..];
    let li = &list[list
        .find(&format!("<li data-code=\"{CHECK}\""))
        .expect("li for check")..];
    let li = &li[..li.find("</li>").unwrap()];
    let from = li.find("data-sentence=\"").unwrap() + "data-sentence=\"".len();
    let rest = &li[from..];
    rest[..rest.find('"').unwrap()].to_owned()
}

#[tokio::test]
async fn end_sentences_include_the_unstored_run_as_newest_point() {
    let today = month_day(now());
    // (stored entries, label)
    for (entries, label) in [(2, "two entries"), (0, "first visit")] {
        let html = end_spectrum(entries).await;
        let s = sentence(&html);
        assert!(
            s.ends_with(&format!("fail since {today}")),
            "{label}: sentence {s:?}"
        );
    }
}

#[tokio::test]
async fn stepper_renders_only_with_two_or_more_runs() {
    // (stored entries, stepper expected)
    for (entries, want) in [(0, false), (1, false), (2, true)] {
        let (_dir, st) = store(entries).await;
        let resp = app(st)
            .oneshot(
                Request::get(format!("/{DOMAIN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{entries} entries");
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8_lossy(&bytes);
        assert_eq!(
            html.contains("id=\"spectrum-stepper\""),
            want,
            "{entries} entries"
        );
    }
}
