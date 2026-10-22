//! Review findings on v2-inspector-shell Phase 2 (requirements 8 and 9): the live list links only
//! blocks the page shows, the DNS band (run on the registrable domain) links none of the query's
//! blocks, a one-line band reads in the singular, and band line names never break mid-token.
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_engine::{Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::BlockId;
use netray_model::{CheckId, CheckResult, Protocol, Status};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

struct Stub(Protocol, Vec<CheckResult>);

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let job = Job::new(vec![JobPart {
            label: "part".into(),
            output: Box::pin(async { Ok(vec![]) }),
        }]);
        let checks = self.1.clone();
        Ok(job.with_band(self.0, Box::pin(async move { Ok(checks) })))
    }
}

fn result(id: &str, status: Status, findings: &[&str]) -> CheckResult {
    CheckResult {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: findings.iter().map(|s| s.to_string()).collect(),
        evidence: Vec::new(),
    }
}

fn ids(p: Protocol) -> Vec<String> {
    catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == p)
        .map(|e| e.id.to_string())
        .collect()
}

fn unesc(s: &str) -> String {
    s.replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&#38;", "&")
}

async fn post(app: &Router, path: &str, body: &str) -> Vec<(String, String)> {
    let resp = app
        .clone()
        .oneshot(
            Request::post(path)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut events = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        events.push((
            j["id"].as_str().unwrap_or_default().to_owned(),
            j["html"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    events
}

fn band_html(events: &[(String, String)]) -> String {
    events
        .iter()
        .find(|e| e.0 == "inspect-band")
        .unwrap_or_else(|| panic!("no inspect-band fragment in {events:?}"))
        .1
        .clone()
}

/// The band fragment of a stub job for `p`, read through the tool's run route.
async fn stub_band(p: Protocol, tool: &str, body: &str, checks: Vec<CheckResult>) -> String {
    let app = inspect_router(InspectDeps::new(
        Arc::new(Stub(p, checks)),
        InspectConfig::default(),
    ));
    band_html(&post(&app, &format!("/inspect/{tool}/run"), body).await)
}

/// The checks section of a band fragment.
fn checks_section(band: &str) -> &str {
    let start = band
        .find("inspector-checks")
        .map(|i| band[..i].rfind("<section").expect("section tag"))
        .unwrap_or_else(|| panic!("no section.inspector-checks in {band}"));
    &band[start..]
}

fn rows(section: &str) -> Vec<&str> {
    section
        .split(r#"<article class="check-row""#)
        .skip(1)
        .collect()
}

fn text_between<'a>(s: &'a str, open: &str, close: &str) -> &'a str {
    let a = s.find(open).unwrap_or_else(|| panic!("no {open} in {s}")) + open.len();
    let b = a + s[a..].find(close).unwrap();
    &s[a..b]
}

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _: &'a str,
        _: IpAddr,
        _: RunTrigger,
        _: bool,
    ) -> BoxFuture<'a, RunStart> {
        unreachable!("no run is started by the DNS inspector")
    }
}

struct Recorder {
    inner: Box<dyn Module>,
    domains: Arc<Mutex<Vec<String>>>,
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
            self.inner.run(ctx, facts).await
        })
    }
}

/// Every `#ev-<id>` target the html links.
fn ev_links(html: &str) -> Vec<String> {
    html.split("href=\"#ev-")
        .skip(1)
        .filter_map(|r| r.split('"').next())
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn the_live_list_links_only_blocks_the_page_shows() {
    let tls = ids(Protocol::Tls);
    let mut checks: Vec<CheckResult> = tls.iter().map(|id| result(id, Status::Pass, &[])).collect();
    checks[0].evidence = vec![BlockId::parse("tls.chain").unwrap()];
    let band = stub_band(Protocol::Tls, "tls", "h=example.com", checks).await;
    assert!(ev_links(&band).is_empty(), "{band}");
}

#[tokio::test]
async fn the_dns_band_links_none_of_the_query_blocks() {
    let sse = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/contracts/prism.sse"),
    )
    .unwrap();
    let recorder = Recorder {
        inner: netray_dns::testing::golden_module(&sse),
        domains: Arc::new(Mutex::new(Vec::new())),
    };
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        dns_module: Some(Arc::new(recorder)),
        ..Default::default()
    });
    let app = inspect_router(InspectDeps::new(
        Arc::new(InspectJobs::new(state)),
        InspectConfig::default(),
    ));
    let events = post(&app, "/inspect/dns/run", "q=www.example.com+A").await;
    let band = band_html(&events);
    let section = checks_section(&band);
    assert!(rows(section).len() > 1, "{section}");
    assert!(
        ev_links(section).is_empty(),
        "the DNS band judges example.com, the page shows www.example.com's records: {section}"
    );
}

#[tokio::test]
async fn a_one_line_band_reads_in_the_singular() {
    let ip = ids(Protocol::Ip);
    assert_eq!(ip.len(), 1, "the IP band is one line in today's catalogue");
    let pass = ip.iter().map(|id| result(id, Status::Pass, &[])).collect();
    let band = stub_band(Protocol::Ip, "ip", "ip=192.0.2.24", pass).await;
    let section = checks_section(&band);
    assert_eq!(
        text_between(section, r#"<h2 class="inspector-checks__title">"#, "</h2>"),
        "The line passes"
    );
    let caption = text_between(
        &band,
        r#"<figcaption class="inspector-band__caption">"#,
        "</figcaption>",
    );
    let code = catalogue()
        .get(&CheckId::parse(&ip[0]).unwrap())
        .map(|_| ())
        .and(netray_web::pages::result::views::line_code(
            &CheckId::parse(&ip[0]).unwrap(),
        ))
        .expect("a line code");
    assert_eq!(
        caption,
        format!(
            "The IP band of the spectrum, zoomed: the same line as position {code} on the result page."
        )
    );
}

#[test]
fn band_line_names_never_break_mid_token() {
    let css = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("static/inspector_shell.css"),
    )
    .unwrap();
    let rule = css
        .split('}')
        .find(|r| {
            r.split('{')
                .next()
                .is_some_and(|s| s.trim() == ".inspector-band__line")
        })
        .expect("an .inspector-band__line rule");
    assert!(!rule.contains("anywhere"), "{rule}");
    assert!(!rule.contains("break-all"), "{rule}");
    assert!(rule.contains("min-width: min-content"), "{rule}");
}
