//! V2 spec v2-inspectors, Phase 7, third review: a DNS job's warnings are shown beside its
//! blocks, and the DNS band runs under the route's `[inspect.dns] deadline_ms`.
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
    /// Milliseconds from the run's start to its context's deadline.
    budget_ms: Arc<std::sync::Mutex<Option<u128>>>,
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
            *self.budget_ms.lock().unwrap() = Some(
                ctx.deadline
                    .saturating_duration_since(std::time::Instant::now())
                    .as_millis(),
            );
            self.inner.run(ctx, facts).await
        })
    }
}

type Budget = Arc<std::sync::Mutex<Option<u128>>>;

fn app(inspect: InspectConfig) -> (Router, Budget) {
    let sse = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/contracts/prism.sse"),
    )
    .unwrap();
    let budget: Budget = Arc::default();
    let recorder = Recorder {
        inner: netray_dns::testing::golden_module(&sse),
        runs: Arc::new(AtomicUsize::new(0)),
        budget_ms: budget.clone(),
    };
    let extractor = Arc::new(IpExtractor::new(&[PROXY.to_string()]));
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(ApiParts {
            dns: Some(netray_dns::testing::stub_jobs()),
            dns_module: Some(Arc::new(recorder)),
            ..Default::default()
        });
    let deps = InspectDeps::new(Arc::new(InspectJobs::new(state)), inspect)
        .with_ip_extractor(extractor);
    (inspect_router(deps), budget)
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
async fn a_parser_warning_is_shown_beside_the_blocks() {
    let (app, _) = app(InspectConfig::default());

    let (status, s) = post_run(&app, "example.com A @nosuchserver").await;

    assert_eq!(status, 200, "{s}");
    assert!(s.contains("id=\"ev-dns.a\""), "{s}");
    assert!(s.contains("unrecognized server: @nosuchserver"), "{s}");
}

#[tokio::test]
async fn the_band_runs_under_the_configured_dns_deadline() {
    let mut inspect = InspectConfig::default();
    inspect.dns.deadline_ms = 90_000;
    let (app, budget) = app(inspect);

    let (status, s) = post_run(&app, "example.com A").await;

    assert_eq!(status, 200, "{s}");
    let ms = budget.lock().unwrap().expect("the band ran");
    assert!(ms > 60_000, "the band's deadline was {ms} ms away, not the route's 90 000");
}
