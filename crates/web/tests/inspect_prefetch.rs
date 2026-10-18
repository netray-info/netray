//! C9: a prefetching or prerendering browser never starts an inspector job.

use std::{
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

#[derive(Default)]
struct Stub {
    admits: Mutex<usize>,
    starts: Mutex<usize>,
}

impl InspectorJobs for Stub {
    fn admit(&self, _tool: Tool, _client: IpAddr, _input: &str) -> Result<(), Duration> {
        *self.admits.lock().unwrap() += 1;
        Ok(())
    }

    fn start(&self, _req: JobRequest) -> Result<Job, String> {
        *self.starts.lock().unwrap() += 1;
        Ok(Job::new(vec![]))
    }
}

#[tokio::test]
async fn prefetching_post_is_503_no_store_and_starts_nothing() {
    for purpose in ["prerender", "prefetch", "prefetch;prerender"] {
        let stub = Arc::new(Stub::default());
        let app = inspect_router(InspectDeps::new(stub.clone(), InspectConfig::default()));
        let req = Request::post("/inspect/http/run")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header("sec-purpose", purpose)
            .body(Body::from("url=https://example.com/"))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();

        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE, "{purpose}");
        assert_eq!(
            res.headers()[header::CACHE_CONTROL],
            "no-store",
            "{purpose}"
        );
        assert_eq!(
            *stub.admits.lock().unwrap(),
            0,
            "admit called for {purpose}"
        );
        assert_eq!(
            *stub.starts.lock().unwrap(),
            0,
            "start called for {purpose}"
        );
    }
}
