//! V2 spec v2-inspectors, requirement 10 (C5, C10): a job with a band streams the protocol's
//! checks after the parts and before `end`; a job without one streams no band.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

type Band = Result<Vec<CheckResult>, String>;

enum Plan {
    NoBand,
    Band(Band),
    SlowBand,
}

struct Stub(Plan);

fn chain_block() -> Block {
    Block {
        id: BlockId::parse("tls.chain@192.0.2.1:443").unwrap(),
        title: "Chain".into(),
        shape: Shape::Chain(vec![vec![(
            "subject".into(),
            Value::Text("leaf.example.com".into()),
        )]]),
    }
}

fn check(id: &str, status: Status) -> CheckResult {
    CheckResult {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: Vec::new(),
        evidence: Vec::new(),
    }
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }

    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let parts = (0..2)
            .map(|n| JobPart {
                label: format!("part{n}"),
                output: Box::pin(async move { Ok(vec![chain_block()]) }),
            })
            .collect();
        let job = Job::new(parts);
        Ok(match &self.0 {
            Plan::NoBand => job,
            Plan::Band(b) => {
                let b = b.clone();
                job.with_band(Protocol::Tls, Box::pin(async move { b }))
            }
            Plan::SlowBand => job.with_band(
                Protocol::Tls,
                Box::pin(async {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    Ok(vec![])
                }),
            ),
        })
    }
}

fn app(plan: Plan, deadline_ms: Option<u64>) -> Router {
    let mut config = InspectConfig::default();
    if let Some(ms) = deadline_ms {
        config.tls.deadline_ms = ms;
    }
    inspect_router(InspectDeps::new(Arc::new(Stub(plan)), config))
}

/// (event name, fragment id, html) per event.
async fn run(app: &Router) -> Vec<(String, String, String)> {
    let resp = app
        .clone()
        .oneshot(
            Request::post("/inspect/tls/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("h=example.com"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut events = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let (mut name, mut data) = (String::new(), String::new());
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

fn band_html(events: &[(String, String, String)]) -> String {
    events
        .iter()
        .find(|e| e.1 == "inspect-band")
        .unwrap_or_else(|| panic!("no inspect-band fragment in {events:?}"))
        .2
        .clone()
}

#[tokio::test]
async fn c5_band_streams_after_the_parts_and_before_end() {
    let ok = vec![check("tls.not_expired", Status::Pass)];
    let events = run(&app(Plan::Band(Ok(ok)), None)).await;

    assert_eq!(events[0].1, "inspect-result");
    assert!(
        events[0].2.contains(r#"<div id="inspect-band""#),
        "{}",
        events[0].2
    );
    let ids: Vec<&str> = events.iter().map(|e| e.1.as_str()).collect();
    let band = ids
        .iter()
        .position(|i| *i == "inspect-band")
        .expect("band fragment");
    for n in 0..2 {
        let part = ids
            .iter()
            .position(|i| *i == format!("inspect-part-{n}"))
            .expect("part fragment");
        assert!(part < band, "{ids:?}");
    }
    assert_eq!(events.len(), band + 2, "{ids:?}");
    assert_eq!(events.last().unwrap().0, "end");
}

#[tokio::test]
async fn c5_job_without_band_streams_no_band_fragment() {
    let events = run(&app(Plan::NoBand, None)).await;

    assert!(events.iter().all(|e| e.1 != "inspect-band"), "{events:?}");
    assert!(!events[0].2.contains("inspect-band"), "{}", events[0].2);
    assert_eq!(events.last().unwrap().0, "end");
}

#[tokio::test]
async fn c5_band_error_text_is_rendered_in_the_band() {
    let events = run(&app(Plan::Band(Err("no A or AAAA record".into())), None)).await;

    assert!(band_html(&events).contains("no A or AAAA record"));
    assert_eq!(events.last().unwrap().0, "end");
}

#[tokio::test]
async fn c5_band_past_the_deadline_says_timed_out_and_the_stream_ends() {
    let events = run(&app(Plan::SlowBand, Some(100))).await;

    assert!(band_html(&events).contains("timed out"));
    assert_eq!(events.last().unwrap().0, "end");
}

#[tokio::test]
async fn c10_chain_block_in_the_part_and_failing_check_in_the_band() {
    let checks = vec![check("tls.not_expired", Status::Fail)];
    let events = run(&app(Plan::Band(Ok(checks)), None)).await;

    let parts: String = events
        .iter()
        .filter(|e| e.1.starts_with("inspect-part-"))
        .map(|e| e.2.as_str())
        .collect();
    assert!(
        parts.contains(r#"id="ev-tls.chain@192.0.2.1:443""#),
        "{parts}"
    );
    let band = band_html(&events);
    assert!(band.contains(r#"data-code="tls.not_expired""#), "{band}");
    assert!(band.contains(r#"data-tone="fail""#), "{band}");
    for code in band.split(r#"data-code=""#).skip(1) {
        assert!(code.starts_with("tls."), "{code}");
    }
    assert!(!band.contains("grade"), "{band}");
}
