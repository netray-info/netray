//! A refused inspector job replaces the back-link too (V2 spec v2-inspectors, requirement 12;
//! review of the feature): a validation refusal, a 429 and a busy answer each send the
//! `inspect-backlink` fragment for the refused target, so the page never keeps the link of an
//! earlier target.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

struct Refuse {
    admit: Result<(), Duration>,
}

impl InspectorJobs for Refuse {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        self.admit
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        Err("refused by validation".to_string())
    }
}

async fn run(app: &Router, tool: &str, form: &str) -> String {
    let req = Request::post(format!("/inspect/{tool}/run"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d).ok())
        .filter_map(|v| v["html"].as_str().map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `inspect-backlink` fragment of a stream, if it carries one.
fn backlink(html: &str) -> Option<&str> {
    let start = html.find("<p id=\"inspect-backlink\">")?;
    let end = html[start..].find("</p>")? + start;
    Some(&html[start..end])
}

#[tokio::test]
async fn a_validation_refusal_sends_the_refused_targets_backlink() {
    let app = inspect_router(InspectDeps::new(
        Arc::new(Refuse { admit: Ok(()) }),
        InspectConfig::default(),
    ));
    let html = run(&app, "tls", "h=mail.example.org%3A143").await;
    assert!(html.contains("refused by validation"), "{html}");
    let link = backlink(&html).unwrap_or_else(|| panic!("no back-link fragment: {html}"));
    assert!(link.contains("href=\"/example.org\""), "{link}");

    let html = run(&app, "tls", "h=192.0.2.1").await;
    let link = backlink(&html).unwrap_or_else(|| panic!("no back-link fragment: {html}"));
    assert!(
        !link.contains("href="),
        "an IP literal clears the back-link: {link}"
    );
}

#[tokio::test]
async fn a_rate_limited_answer_sends_the_refused_targets_backlink() {
    let app = inspect_router(InspectDeps::new(
        Arc::new(Refuse {
            admit: Err(Duration::from_secs(30)),
        }),
        InspectConfig::default(),
    ));
    let html = run(&app, "http", "url=https%3A%2F%2Fwww.example.net%2F").await;
    let link = backlink(&html).unwrap_or_else(|| panic!("no back-link fragment: {html}"));
    assert!(link.contains("href=\"/example.net\""), "{link}");
}
