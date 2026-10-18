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
        Err("stub".into())
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

fn backlink(body: &str) -> &str {
    let start = body
        .find("id=\"inspect-backlink\"")
        .expect("the page holds the back-link element");
    let end = body[start..].find("</p>").expect("the element closes") + start;
    &body[start..end]
}

#[tokio::test]
async fn a_target_that_is_not_a_host_name_gets_no_backlink() {
    for uri in [
        "/inspect/dns?q=https://example.com",
        "/inspect/dns?q=www.example.com/",
        "/inspect/tls?h=www.example.com/",
        "/inspect/tls?h=exa%20mple.com",
    ] {
        let body = page(uri).await;
        assert!(
            !backlink(&body).contains("href="),
            "{uri}: {}",
            backlink(&body)
        );
    }
    let body = page("/inspect/dns?q=www.example.com").await;
    assert!(backlink(&body).contains("href=\"/example.com\""));
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
