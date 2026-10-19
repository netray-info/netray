//! v2-cutover C12: the inspector page marks `#inspect-backlink` with `data-stream-reset`, so the
//! stream island empties it on submit and a refused job leaves no stale back-link.
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

#[tokio::test]
async fn backlink_element_is_marked_for_stream_reset() {
    for uri in ["/inspect/tls", "/inspect/tls?h=www.example.com"] {
        let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
        let res = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "GET {uri}");
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&bytes).into_owned();
        let tag = body
            .split("id=\"inspect-backlink\"")
            .nth(1)
            .and_then(|rest| rest.split('>').next())
            .unwrap_or_else(|| panic!("no #inspect-backlink on {uri}"));
        assert!(
            tag.contains("data-stream-reset"),
            "#inspect-backlink on {uri} lacks data-stream-reset: {tag}"
        );
    }
}
