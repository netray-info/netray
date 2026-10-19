//! V2 spec v2-inspectors, requirement 19 (C2, C7, C9): the DNS inspector's band is the DNS
//! module's checks on the registrable domain of the queried name, streamed after the parts and
//! before `end`; a name without a registrable domain gets a sentence and no module run.
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::{Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
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
        unreachable!("no run is started by the DNS inspector")
    }
}

/// Delegates to the golden module and records `ctx.domain` of every run.
struct Recorder {
    inner: Box<dyn Module>,
    domains: Arc<Mutex<Vec<String>>>,
    delay: Duration,
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
            self.domains
                .lock()
                .unwrap()
                .push(ctx.domain.as_str().to_owned());
            tokio::time::sleep(self.delay).await;
            self.inner.run(ctx, facts).await
        })
    }
}

fn app(delay: Duration, deadline_ms: Option<u64>) -> (Router, Arc<Mutex<Vec<String>>>) {
    let sse = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/contracts/prism.sse"),
    )
    .unwrap();
    let domains = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder {
        inner: netray_dns::testing::golden_module(&sse),
        domains: domains.clone(),
        delay,
    };
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        dns_module: Some(Arc::new(recorder)),
        ..Default::default()
    });
    let mut config = InspectConfig::default();
    if let Some(ms) = deadline_ms {
        config.dns.deadline_ms = ms;
    }
    let deps = InspectDeps::new(Arc::new(InspectJobs::new(state)), config);
    (inspect_router(deps), domains)
}

/// (event name, fragment id, html) per event.
async fn run(app: &Router, q: &str) -> Vec<(String, String, String)> {
    let body = format!("q={}", q.replace(' ', "+"));
    let resp = app
        .clone()
        .oneshot(
            Request::post("/inspect/dns/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut events = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let (mut name, mut data) = (String::new(), String::new());
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim().to_owned();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        events.push((
            name,
            j["id"].as_str().unwrap_or_default().to_owned(),
            j["html"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    events
}

fn band_html(events: &[(String, String, String)]) -> String {
    events
        .iter()
        .find(|e| e.1 == "inspect-band")
        .unwrap_or_else(|| panic!("no inspect-band fragment in {events:?}"))
        .2
        .clone()
}

#[tokio::test]
async fn c7_band_runs_the_module_once_on_the_registrable_domain() {
    let (app, domains) = app(Duration::ZERO, None);

    let events = run(&app, "_dmarc.example.com TXT").await;

    assert_eq!(*domains.lock().unwrap(), vec!["example.com".to_owned()]);
    let band = band_html(&events);
    assert!(band.contains(r#"data-code="dns."#), "{band}");
    for code in band.split(r#"data-code=""#).skip(1) {
        assert!(code.starts_with("dns."), "{code}");
    }
    assert_eq!(events.last().unwrap().0, "end");
}

#[tokio::test]
async fn c9_name_without_registrable_domain_gets_a_sentence_and_no_run() {
    let (app, domains) = app(Duration::ZERO, None);

    let events = run(&app, "com NS").await;

    assert!(band_html(&events).contains("No registrable domain"));
    assert!(domains.lock().unwrap().is_empty(), "the module ran");
    assert_eq!(events.last().unwrap().0, "end");
}

#[tokio::test]
async fn c2_band_streams_after_the_parts_and_before_end() {
    let (app, _) = app(Duration::ZERO, None);

    let events = run(&app, "example.com A").await;

    let ids: Vec<&str> = events.iter().map(|e| e.1.as_str()).collect();
    let band = ids
        .iter()
        .position(|i| *i == "inspect-band")
        .expect("band fragment");
    let part = ids
        .iter()
        .position(|i| i.starts_with("inspect-part-"))
        .expect("part fragment");
    assert!(part < band, "{ids:?}");
    assert_eq!(events.len(), band + 2, "{ids:?}");
    assert_eq!(events.last().unwrap().0, "end");
}

#[tokio::test]
async fn c2_slow_module_past_the_deadline_says_timed_out_and_the_stream_ends() {
    let (app, _) = app(Duration::from_millis(500), Some(100));

    let events = run(&app, "example.com A").await;

    assert!(band_html(&events).contains("timed out"));
    assert_eq!(events.last().unwrap().0, "end");
}
