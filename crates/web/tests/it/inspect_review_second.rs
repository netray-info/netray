//! Second review of the inspector frame (V2 spec v2-inspectors, requirements 9 and 12): a target
//! that is not a host name gets no back-link, and the stream form carries the error element the
//! `stream` island unhides (frontend rules §5).
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        Ok(Job::new(vec![]))
    }
}

async fn page(uri: &str) -> String {
    let res = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()))
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "GET {uri}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The start fragment of a posted run (v2-inspector-shell requirement 6: the back link moved from
/// the page into the run's head).
async fn start(tool: &str, field: &str, value: &str) -> String {
    let body = format!(
        "{field}={}",
        percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC)
    );
    let res = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()))
        .oneshot(
            Request::post(format!("/inspect/{tool}/run"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let data = text
        .lines()
        .find_map(|l| l.strip_prefix("data:"))
        .expect("a first fragment");
    let j: serde_json::Value = serde_json::from_str(data.trim()).unwrap();
    j["html"].as_str().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn a_target_that_is_not_a_host_name_gets_no_backlink() {
    for (tool, field, value) in [
        ("dns", "q", "https://example.com"),
        ("dns", "q", "www.example.com/"),
        ("tls", "h", "www.example.com/"),
        ("tls", "h", "exa mple.com"),
    ] {
        let html = start(tool, field, value).await;
        assert!(
            !html.contains("inspector-backlink"),
            "{tool} {value}: {html}"
        );
    }
    let html = start("dns", "q", "www.example.com").await;
    assert!(
        html.contains(r#"class="inspector-backlink" href="/example.com""#),
        "{html}"
    );
}

#[tokio::test]
async fn the_stream_form_carries_the_hidden_error_element() {
    for tool in ["dns", "tls", "http", "ip"] {
        let body = page(&format!("/inspect/{tool}")).await;
        let form_start = body
            .find("data-island=\"stream\"")
            .unwrap_or_else(|| panic!("{tool}: no stream form"));
        let form_end = body[form_start..].find("</form>").expect("form closes") + form_start;
        let form = &body[form_start..form_end];
        let error = form
            .find("data-stream-error")
            .unwrap_or_else(|| panic!("{tool}: no [data-stream-error] inside the form"));
        let tag_end = form[error..].find('>').unwrap() + error;
        assert!(
            form[..tag_end]
                .rsplit('<')
                .next()
                .unwrap()
                .contains("hidden"),
            "{tool}: the error element starts hidden"
        );
    }
}
