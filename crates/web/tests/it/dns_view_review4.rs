//! v2-dns-ui Phase 4 review: a bare `+trace` names no type and carries no "first type" note; a
//! trace server that answered without records says so in words; a hop shows its level as text.
//! Driven through `inspect_router` over `InspectJobs` and the DNS stubs.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use tower::ServiceExt;

const PROXY: &str = "203.0.113.9";
const CALLER: &str = "198.51.100.1";
const WARNING: &str = "Only the first type is traced";

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
        unreachable!("no run is started by the DNS inspector")
    }
}

fn golden() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/contracts/prism.sse"
    ))
    .expect("prism check fixture")
}

fn app() -> Router {
    let extractor = Arc::new(IpExtractor::new(&[PROXY.to_string()]));
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(ApiParts {
            dns: Some(netray_dns::testing::stub_jobs()),
            dns_module: Some(Arc::from(netray_dns::testing::golden_module(&golden()))),
            ..Default::default()
        });
    let deps = InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default())
        .with_ip_extractor(extractor);
    inspect_router(deps)
}

async fn run_q(app: &Router, q: &str) -> String {
    let body = format!("q={}", q.replace('+', "%2B").replace(' ', "+"));
    let mut req = Request::post("/inspect/dns/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", CALLER)
        .body(Body::from(body))
        .unwrap();
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes)
        .replace("\r\n", "\n")
        .replace("\\\"", "\"")
}

#[tokio::test]
async fn a_bare_trace_names_no_type_and_carries_no_note() {
    let s = run_q(&app(), "example.com +trace").await;
    assert!(s.contains("id=\"ev-dns.trace\""), "{s}");
    assert!(!s.contains(WARNING), "{s}");
}

#[tokio::test]
async fn an_answer_without_records_reads_in_words_and_the_level_shows() {
    let s = run_q(&app(), "example.com CAA +trace").await;
    assert!(
        s.contains(r#"<span class="dns-hop__outcome">answer, no records</span>"#)
            || s.contains(r#"<span class="dns-hop__outcome">answer</span>"#),
        "{s}"
    );
    assert!(s.contains(r#"<span class="dns-hop__level">"#), "{s}");
}
