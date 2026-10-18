//! The DNS inspector page (V2 spec v2-inspectors, Phase 7, requirements 19-20): `GET /inspect/dns`
//! carries the dns picker island, and a `+check` query is explained without starting a job.
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

/// The element carrying `data-island="dns"`, from its open tag to its matching close tag.
fn dns_island(body: &str) -> String {
    let attr = body
        .find(r#"data-island="dns""#)
        .unwrap_or_else(|| panic!("no data-island=\"dns\" in {body}"));
    let start = body[..attr].rfind('<').expect("open tag");
    let name: String = body[start + 1..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut depth = 0usize;
    let mut i = start;
    while i < body.len() {
        let rest = &body[i..];
        if rest.starts_with(&close) {
            depth -= 1;
            i += close.len();
            if depth == 0 {
                return body[start..i].to_string();
            }
        } else if rest.starts_with(&open)
            && rest[open.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace() || c == '>')
        {
            depth += 1;
            i += open.len();
        } else {
            i += rest.chars().next().unwrap().len_utf8();
        }
    }
    panic!("island element not closed in {body}");
}

#[tokio::test]
async fn c10_a_check_flag_is_explained_and_starts_no_job() {
    let (app, stub) = app();
    let (status, body) = get(&app, "/inspect/dns?q=example.com%20%2Bcheck").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("+check is not a separate job: the DNS band covers its checks."),
        "{body}"
    );
    assert!(stub.admits.lock().unwrap().is_empty(), "admit called");
    assert!(stub.starts.lock().unwrap().is_empty(), "start called");
}

#[tokio::test]
async fn c3_the_dns_page_carries_the_picker_island() {
    let (app, _stub) = app();
    let (status, body) = get(&app, "/inspect/dns").await;

    assert_eq!(status, StatusCode::OK);
    let island = dns_island(&body);
    for needle in [
        r#"<input type="checkbox" name="dns-type" value="A""#,
        r#"<input type="checkbox" name="dns-type" value="MX""#,
        r#"name="dns-job""#,
        r#"value="trace""#,
        r#"value="dnssec""#,
        r#"<select name="dns-resolver""#,
    ] {
        assert!(island.contains(needle), "missing {needle} in {island}");
    }
    assert!(
        island.contains(r#"type="radio" name="dns-job""#)
            || island.contains(r#"name="dns-job" type="radio""#),
        "dns-job is not a radio: {island}"
    );
    assert!(!island.contains("style="), "inline style in {island}");
}

#[tokio::test]
async fn c3_the_tls_page_carries_no_dns_island() {
    let (app, _stub) = app();
    let (status, body) = get(&app, "/inspect/tls").await;

    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains(r#"data-island="dns""#), "{body}");
}
