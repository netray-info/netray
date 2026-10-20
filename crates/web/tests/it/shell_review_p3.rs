//! Review finding on v2-inspector-shell Phase 3 (requirement 13, Rule M): a key only the API's DNS
//! result cache holds, whose result carries no block, renders the part's reason, never an empty
//! section.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_dns::{DnsCachedJob, DnsCachedResult, DnsResultCache, check_result};
use netray_engine::SectionOutcome;
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
async fn an_api_result_without_blocks_shows_a_reason_not_an_empty_part() {
    let cache = DnsResultCache::new();
    let key = DnsResultCache::fresh_key();
    cache
        .insert(DnsCachedResult {
            key: key.clone(),
            mode: "check".to_owned(),
            query: "example.com".to_owned(),
            result: DnsCachedJob::Check(check_result(SectionOutcome::TimedOut)),
        })
        .await;
    let app = inspect_router(
        InspectDeps::new(Arc::new(Stub), InspectConfig::default()).with_dns_results(cache),
    );
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/inspect/dns?r={key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let page =
        String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).into_owned();
    let section = page
        .split(r#"id="inspect-part-0""#)
        .nth(1)
        .and_then(|s| s.split("</section>").next())
        .expect("the part section");
    assert!(
        section.contains("<p>"),
        "the part names why it holds no evidence: {section}"
    );
}
