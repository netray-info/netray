//! V2 spec v2-inspectors, requirement 10 (C8, C2): `POST /inspect/<tool>/run` answers an event
//! stream: the run line first, then one fragment per finished part, then `end`; invalid input is
//! one fragment with the job function's message.
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use netray_model::evidence::{Block, BlockId, Shape};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

struct Stub {
    seen: Mutex<Vec<JobRequest>>,
    refuse: Option<&'static str>,
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }

    fn start(&self, req: JobRequest) -> Result<Job, String> {
        self.seen.lock().unwrap().push(req);
        if let Some(msg) = self.refuse {
            return Err(msg.into());
        }
        let chain = Block {
            id: BlockId::parse("tls.chain").unwrap(),
            title: "Chain".into(),
            shape: Shape::Chain(vec![vec![(
                "subject".into(),
                netray_model::evidence::Value::Text("leaf.example.com".into()),
            )]]),
        };
        let handshake = Block {
            id: BlockId::parse("tls.handshake").unwrap(),
            title: "Handshake".into(),
            shape: Shape::Fields(vec![(
                "protocol".into(),
                netray_model::evidence::Value::Text("TLSv1.3".into()),
            )]),
        };
        let part = |label: &str, block: Block| JobPart {
            label: label.into(),
            output: Box::pin(async move { Ok(vec![block]) }),
        };
        Ok(Job::new(vec![
            part("chain", chain),
            part("handshake", handshake),
        ]))
    }
}

fn app(refuse: Option<&'static str>) -> (Router, Arc<Stub>) {
    let stub = Arc::new(Stub {
        seen: Mutex::new(Vec::new()),
        refuse,
    });
    let deps = InspectDeps::new(stub.clone(), InspectConfig::default());
    (inspect_router(deps), stub)
}

/// (event name, id, html) per event; `end` carries no data.
async fn post(app: &Router, body: &str) -> (StatusCode, String, Vec<(String, String, String)>) {
    let resp = app
        .clone()
        .oneshot(
            Request::post("/inspect/tls/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let ctype = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
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
    (status, ctype, events)
}

#[tokio::test]
async fn c8_run_streams_the_run_line_then_each_block_then_end() {
    let (app, _) = app(None);

    let (status, ctype, events) = post(&app, "h=example.com").await;

    assert_eq!(status, StatusCode::OK);
    assert!(ctype.starts_with("text/event-stream"), "{ctype}");
    let (name, id, html) = &events[0];
    assert_eq!((name.as_str(), id.as_str()), ("fragment", "inspect-result"));
    assert!(
        html.contains(r#"<span class="run-line__part">live</span>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<span class="run-line__part">not recorded</span>"#),
        "{html}"
    );
    assert_eq!(events.last().unwrap().0, "end");
    let later: String = events[1..].iter().map(|e| e.2.as_str()).collect();
    assert!(later.contains(r#"id="ev-tls.chain""#), "{later}");
    assert!(later.contains(r#"id="ev-tls.handshake""#), "{later}");
    for (_, _, html) in &events {
        assert!(!html.contains(r#"class="grade"#), "{html}");
    }
}

#[tokio::test]
async fn c2_invalid_input_is_one_fragment_with_the_message_and_the_field_unchanged() {
    let (app, stub) = app(Some("not a host name"));

    let (_, _, events) = post(&app, "h=Not%20A%20Host&trigger=load").await;

    let names: Vec<&str> = events.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(names, ["fragment", "end"]);
    assert_eq!(events[0].1, "inspect-result");
    assert!(events[0].2.contains("not a host name"), "{}", events[0].2);
    let seen = stub.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].input, "Not A Host");
}
