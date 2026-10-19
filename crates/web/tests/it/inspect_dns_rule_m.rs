//! V2 spec v2-inspectors, Phase 7, operator decision 2026-10-18 (Rule M in the DNS inspector):
//! a record type whose lookups all failed shows "not measured" with its reason, a partly failed
//! query shows both halves, and an error event is shown beside the blocks it follows.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_engine::{Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use tower::ServiceExt;

const PROXY: &str = "203.0.113.9";
const CALLER: &str = "198.51.100.1";

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

/// Delegates to the golden module and counts runs.
struct Recorder {
    inner: Box<dyn Module>,
    runs: Arc<AtomicUsize>,
}

impl Module for Recorder {
    fn protocol(&self) -> Protocol {
        self.inner.protocol()
    }
    fn checks(&self) -> &'static [CheckId] {
        self.inner.checks()
    }
    fn volatile(&self) -> &'static [netray_engine::EvidencePath] {
        self.inner.volatile()
    }
    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            self.runs.fetch_add(1, Ordering::SeqCst);
            self.inner.run(ctx, facts).await
        })
    }
}

fn app(config: ApiConfig) -> (Router, Arc<AtomicUsize>) {
    let sse = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/contracts/prism.sse"),
    )
    .unwrap();
    let runs = Arc::new(AtomicUsize::new(0));
    let recorder = Recorder {
        inner: netray_dns::testing::golden_module(&sse),
        runs: runs.clone(),
    };
    let extractor = Arc::new(IpExtractor::new(&[PROXY.to_string()]));
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(ApiParts {
            dns: Some(netray_dns::testing::stub_jobs()),
            dns_module: Some(Arc::new(recorder)),
            ..Default::default()
        });
    let deps = InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default())
        .with_ip_extractor(extractor);
    (inspect_router(deps), runs)
}

/// (status, body with unescaped quotes)
async fn post_run(app: &Router, q: &str) -> (StatusCode, String) {
    let body = format!("q={}", q.replace('+', "%2B").replace(' ', "+"));
    let mut req = Request::post("/inspect/dns/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", CALLER)
        .body(Body::from(body))
        .unwrap();
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes)
        .replace("\r\n", "\n")
        .replace("\\\"", "\"");
    (status, text)
}

#[tokio::test]
async fn a_servfail_query_shows_not_measured_never_an_empty_part() {
    let (app, _) = app(ApiConfig::default());

    let (status, s) = post_run(&app, "dnssec-failed.org A").await;

    assert_eq!(status, 200, "{s}");
    assert!(s.contains("id=\"ev-dns.a\""), "{s}");
    assert!(s.contains("not measured"), "{s}");
    assert!(s.contains("SERVFAIL"), "{s}");
}

#[tokio::test]
async fn a_partly_failed_query_shows_the_answer_and_the_failure() {
    let (app, _) = app(ApiConfig::default());

    let (status, s) = post_run(&app, "partial.example.net A AAAA").await;

    assert_eq!(status, 200, "{s}");
    assert!(s.contains("id=\"ev-dns.a\""), "{s}");
    assert!(s.contains("192.0.2.10"), "{s}");
    assert!(s.contains("id=\"ev-dns.aaaa\""), "{s}");
    assert!(s.contains("not measured"), "{s}");
}

#[tokio::test]
async fn an_error_event_after_a_block_is_shown() {
    let (app, _) = app(ApiConfig::default());

    let (status, s) = post_run(&app, "nons.example.net +auth").await;

    assert_eq!(status, 200, "{s}");
    assert!(s.contains("id=\"ev-dns.authcompare\""), "{s}");
    assert!(
        s.contains("could not discover authoritative nameservers"),
        "the error event was dropped: {s}"
    );
}
