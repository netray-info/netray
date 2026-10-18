//! V2 spec v2-inspectors requirement 13: inspector jobs reach the routes through one trait,
//! `InspectorJobs`; the inspector routes name no module crate.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, header};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

const FORBIDDEN: [&str; 6] = [
    "netray_dns",
    "netray_tls",
    "netray_http",
    "netray_ip",
    "netray_email",
    "lens::",
];

#[test]
fn inspect_routes_name_no_module_crate() {
    let pages = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/pages");
    let main = pages.join("inspect.rs");
    assert!(main.is_file(), "missing {}", main.display());

    let mut files = vec![main];
    for entry in fs::read_dir(&pages).unwrap() {
        let p = entry.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if p.is_dir() && name == "inspect" {
            for sub in fs::read_dir(&p).unwrap() {
                files.push(sub.unwrap().path());
            }
        } else if p.is_file() && name.starts_with("inspect") && !files.contains(&p) {
            files.push(p);
        }
    }

    for f in files.iter().filter(|f| f.is_file()) {
        let source = fs::read_to_string(f).unwrap();
        for name in FORBIDDEN {
            assert!(
                !source.contains(name),
                "{} names `{name}`; jobs reach the routes only through `InspectorJobs`",
                f.display()
            );
        }
    }
}

#[derive(Default)]
struct Stub {
    starts: Mutex<Vec<JobRequest>>,
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, req: JobRequest) -> Result<Job, String> {
        self.starts.lock().unwrap().push(req);
        Ok(Job::new(vec![]))
    }
}

#[tokio::test]
async fn stub_jobs_receive_the_post_through_the_seam() {
    let stub = Arc::new(Stub::default());
    let jobs: Arc<dyn InspectorJobs> = stub.clone();
    let app = inspect_router(InspectDeps::new(jobs, InspectConfig::default()));

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/inspect/ip/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("ip=192.0.2.1"))
                .unwrap(),
        )
        .await
        .unwrap();
    // Drain the stream so the handler has run to completion.
    let _ = http_body_util::BodyExt::collect(res.into_body()).await;

    let starts = stub.starts.lock().unwrap();
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0].tool, Tool::Ip);
    assert_eq!(starts[0].input, "192.0.2.1");
}
