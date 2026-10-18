//! The inspector page (V2 spec v2-inspectors, Phase 5, requirement 9): `GET /inspect/<tool>`
//! renders the form, prefills and autostarts from the query string, and never starts a job.
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

/// Records every call to the job seam.
#[derive(Default)]
struct Stub {
    admits: Mutex<Vec<String>>,
    starts: Mutex<Vec<String>>,
}

impl InspectorJobs for Stub {
    fn admit(&self, _tool: Tool, _client: IpAddr, input: &str) -> Result<(), Duration> {
        self.admits.lock().unwrap().push(input.to_string());
        Ok(())
    }
    fn start(&self, req: JobRequest) -> Result<Job, String> {
        self.starts.lock().unwrap().push(req.input);
        Err("stub".into())
    }
}

fn app() -> (Router, Arc<Stub>) {
    let stub = Arc::new(Stub::default());
    let router = inspect_router(InspectDeps::new(stub.clone(), InspectConfig::default()));
    (router, stub)
}

async fn get(app: &Router, uri: &str) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn assert_no_job(stub: &Stub, what: &str) {
    assert!(
        stub.admits.lock().unwrap().is_empty(),
        "{what}: admit called"
    );
    assert!(
        stub.starts.lock().unwrap().is_empty(),
        "{what}: start called"
    );
}

#[tokio::test]
async fn c1_each_inspector_page_renders_the_form_without_starting_a_job() {
    let rows = [("dns", "q"), ("tls", "h"), ("http", "url"), ("ip", "ip")];
    for (tool, field) in rows {
        let (app, stub) = app();
        let (status, body) = get(&app, &format!("/inspect/{tool}")).await;

        assert_eq!(status, StatusCode::OK, "{tool}");
        assert!(body.contains("<html"), "{tool}: not a page");
        assert!(body.contains(r#"data-island="stream""#), "{tool}: {body}");
        assert!(
            body.contains(&format!(r#"data-stream-url="/inspect/{tool}/run""#)),
            "{tool}: {body}"
        );
        assert!(
            body.contains(&format!(r#"name="{field}""#)),
            "{tool}: no input {field}"
        );
        assert!(
            body.contains(r#"<div id="inspect-result"></div>"#),
            "{tool}: {body}"
        );
        assert!(body.contains("<noscript>"), "{tool}: no noscript notice");
        assert!(
            !body.contains("data-stream-autostart"),
            "{tool}: autostart without a query"
        );
        assert_no_job(&stub, tool);
    }
}

#[tokio::test]
async fn c7_a_query_prefills_the_input_and_autostarts_without_starting_a_job() {
    let (app, stub) = app();
    let (status, body) = get(&app, "/inspect/tls?h=example.com:443").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"value="example.com:443""#), "{body}");
    assert!(body.contains(r#"data-stream-autostart="load""#), "{body}");
    assert!(
        body.contains(r#"<div id="inspect-result"></div>"#),
        "{body}"
    );
    assert_no_job(&stub, "tls prefill");
}

#[tokio::test]
async fn c17_email_has_no_inspector_page() {
    let (app, stub) = app();
    let (status, body) = get(&app, "/inspect/email").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("<h1>Nothing here</h1>"), "{body}");
    assert_no_job(&stub, "email");
}
