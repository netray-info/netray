//! Review findings on the inspector frame (V2 spec v2-inspectors, requirement 12): the back-link
//! follows the posted target and the page's own target parsing agrees with the job functions'.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
    registrable_domain,
};
use tower::ServiceExt;

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let block = Block {
            id: BlockId::parse("tls.handshake").unwrap(),
            title: "Handshake".into(),
            shape: Shape::Fields(vec![("protocol".into(), Value::Text("TLSv1.3".into()))]),
        };
        Ok(Job::new(vec![JobPart {
            label: "handshake".into(),
            output: Box::pin(async move { Ok(vec![block]) }),
        }]))
    }
}

fn app() -> Router {
    inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()))
}

async fn get(uri: &str) -> String {
    let res = app()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "GET {uri}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// (event name, id, html) per event.
async fn post(body: &str) -> Vec<(String, String, String)> {
    post_to("tls", body).await
}

async fn post_to(tool: &str, body: &str) -> Vec<(String, String, String)> {
    let resp = app()
        .oneshot(
            Request::post(format!("/inspect/{tool}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
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

/// The back link of a stream: the `inspector-backlink` anchor of the start fragment (the first
/// `inspect-result` fragment), or "" when it has none (v2-inspector-shell requirement 6).
fn backlink_html(events: &[(String, String, String)]) -> String {
    let start = events
        .iter()
        .find(|e| e.1 == "inspect-result")
        .map(|e| e.2.clone())
        .expect("an inspect-result fragment");
    match start.find(r#"class="inspector-backlink""#) {
        Some(at) => {
            let from = start[..at].rfind("<a").unwrap_or(at);
            let to = start[at..].find("</a>").map_or(start.len(), |e| at + e + 4);
            start[from..to].to_string()
        }
        None => String::new(),
    }
}

#[tokio::test]
async fn backlink_follows_the_posted_target() {
    let page = get("/inspect/tls").await;
    assert!(
        !page.contains("inspect-backlink") && !page.contains("inspector-backlink"),
        "GET page: {page}"
    );

    let html = backlink_html(&post("h=www.example.com").await);
    assert!(html.contains(r#"href="/example.com""#), "{html}");

    let html = backlink_html(&post("h=192.0.2.1").await);
    assert!(html.is_empty(), "{html}");
}

#[tokio::test]
async fn backslash_ends_the_https_authority() {
    let events = post_to("http", "url=https://bank.example.com%5C@evil.example/").await;
    let html = backlink_html(&events);
    assert!(html.contains(r#"href="/example.com""#), "{html}");
    let page = get("/inspect/http?url=https://bank.example.com%5C@evil.example/").await;
    assert!(!page.contains("/evil.example"), "{page}");
    assert!(!html.contains("evil.example"), "{html}");
}

#[tokio::test]
async fn percent_encoded_port_is_decoded_before_the_split() {
    let html = backlink_html(&post("h=www.example.com%253A443").await);
    assert!(html.contains(r#"href="/example.com""#), "{html}");
}

#[test]
fn registrable_domain_is_case_insensitive() {
    assert_eq!(
        registrable_domain("WWW.EXAMPLE.CO.UK"),
        Some("example.co.uk".to_string())
    );
    assert_eq!(
        registrable_domain("example.co.uk"),
        Some("example.co.uk".to_string())
    );
}
