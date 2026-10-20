//! V2 spec v2-http-ui, requirement 9 (C10): when the live band resolves with checks, a part
//! holding a claimed block is sent again after the band, now as a view with tones.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

struct Hsts;

impl InspectorJobs for Hsts {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let block = Block {
            id: BlockId::parse("http.hsts").unwrap(),
            title: "HSTS".into(),
            shape: Shape::Fields(vec![
                ("max_age".into(), Value::Int(31536000)),
                ("include_subdomains".into(), Value::Bool(true)),
                ("preload".into(), Value::Bool(false)),
            ]),
        };
        let check = CheckResult {
            id: CheckId::parse("http.hsts").unwrap(),
            status: Status::Pass,
            findings: Vec::new(),
            evidence: vec![BlockId::parse("http.hsts").unwrap()],
        };
        let job = Job::new(vec![JobPart {
            label: "hsts".into(),
            output: Box::pin(async move { Ok(vec![block]) }),
        }]);
        Ok(job.with_band(Protocol::Http, Box::pin(async move { Ok(vec![check]) })))
    }
}

#[tokio::test]
async fn c10_live_stream_sends_the_part_again_after_the_band_as_a_view() {
    let app = inspect_router(InspectDeps::new(Arc::new(Hsts), InspectConfig::default()));
    let resp = app
        .oneshot(
            Request::post("/inspect/http/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("url=https%3A%2F%2Fexample.com%2F"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut events: Vec<(String, String)> = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        events.push((
            j["id"].as_str().unwrap_or_default().to_owned(),
            j["html"].as_str().unwrap_or_default().to_owned(),
        ));
    }

    let parts: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.0 == "inspect-part-0")
        .map(|(i, _)| i)
        .collect();
    let band = events
        .iter()
        .position(|e| e.0 == "inspect-band")
        .unwrap_or_else(|| panic!("no inspect-band fragment in {events:?}"));
    assert_eq!(parts.len(), 2, "part fragments: {events:?}");
    assert!(parts[1] > band, "second part before the band: {events:?}");
    let second = &events[parts[1]].1;
    assert!(
        second.contains(r#"<section class="protocol-view" id="ev-http.hsts""#),
        "{second}"
    );
    assert!(second.contains(r#"data-tone="pass""#), "{second}");
}
