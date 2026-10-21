//! V2 spec v2-ip-ui, requirement 2 (C2, C15): the caller page's "Request headers" section lists the
//! X-Forwarded-For hops, client first, then the filtered request headers in received order. Driven
//! through `inspect_router` over `StubIpLookup`, the peer being the trusted proxy.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_ip::IpLookup;
use netray_ip::testing::StubIpLookup;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use tower::ServiceExt;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        unreachable!("no run is started by the inspector")
    }
}

const PROXY: &str = "203.0.113.9";

fn app() -> Router {
    let config: ApiConfig = toml::from_str("[ip]\nfiltered_headers = [\"^x-secret-\"]\n").unwrap();
    let extractor = Arc::new(IpExtractor::new(&[PROXY.to_string()]));
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(ApiParts {
            ip: Some(Arc::new(StubIpLookup::new()) as Arc<dyn IpLookup>),
            ..Default::default()
        });
    inspect_router(
        InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default())
            .with_ip_extractor(extractor),
    )
}

#[tokio::test]
async fn caller_page_lists_forwarded_hops_then_filtered_headers_in_received_order() {
    let mut req = Request::get("/inspect/ip")
        .header("x-forwarded-for", "192.0.2.10, 198.51.100.7")
        .header("x-alpha-marker", "alpha-value-1")
        .header("x-secret-token", "secret-value-9")
        .header("x-beta-marker", "beta-value-2")
        .header("x-gamma-marker", "gamma-value-3")
        .body(Body::empty())
        .unwrap();
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&bytes).into_owned();

    let start = html
        .find("Request headers")
        .unwrap_or_else(|| panic!("no Request headers section: {html}"));
    let section = &html[start..];

    let mut at = 0;
    for needle in [
        "192.0.2.10",
        "198.51.100.7",
        "x-alpha-marker",
        "alpha-value-1",
        "x-beta-marker",
        "beta-value-2",
        "x-gamma-marker",
        "gamma-value-3",
    ] {
        let pos = section[at..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing or out of order: {section}"));
        at += pos + needle.len();
    }
    assert!(!html.contains("x-secret-token"), "{html}");
    assert!(!html.contains("secret-value-9"), "{html}");
}
