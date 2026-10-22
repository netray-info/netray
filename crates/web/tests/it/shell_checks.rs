//! V2 spec v2-inspector-shell, requirement 9 (C2, C6, C8, C9): the live check list under the band.
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

#[tokio::test]
async fn c6_http_csp_warn_row_carries_title_finding_fix_and_guide() {
    let spec = catalogue()
        .get(&CheckId::parse("http.csp").unwrap())
        .unwrap();
    let results = ids(Protocol::Http)
        .iter()
        .map(|id| {
            if id == "http.csp" {
                result(id, Status::Warn, &["style-src contains 'unsafe-inline'"])
            } else {
                result(id, Status::Pass, &[])
            }
        })
        .collect();

    let band = stub_band(
        Protocol::Http,
        "http",
        "url=https%3A%2F%2Fexample.com%2F",
        results,
    )
    .await;

    let section = checks_section(&band);
    let row = rows(section)
        .into_iter()
        .find(|r| unesc(r).contains(&format!(">{}</h3>", spec.title)))
        .unwrap_or_else(|| panic!("no CSP row in {section}"));
    let row = unesc(row);
    assert!(row.starts_with(r#" data-tone="warn""#), "{row}");
    assert!(row.contains("style-src contains 'unsafe-inline'"), "{row}");
    assert!(row.contains(&spec.fix_hint), "{row}");
    assert!(row.contains(&format!(r#"href="{}""#, spec.guide)), "{row}");
}

#[tokio::test]
async fn c2_rows_follow_the_catalogue_with_tone_fallback_finding_and_fix() {
    let kinds = [Status::Pass, Status::Warn, Status::Fail];
    let tones = ["pass", "warn", "fail"];
    let want_ids = ids(Protocol::Tls);
    let results = want_ids
        .iter()
        .enumerate()
        .map(|(i, id)| result(id, kinds[i % 3].clone(), &[]))
        .collect();

    let band = stub_band(Protocol::Tls, "tls", "h=example.com", results).await;

    let section = checks_section(&band);
    let got = rows(section);
    assert_eq!(got.len(), want_ids.len(), "{section}");
    for (i, (row, id)) in got.iter().zip(&want_ids).enumerate() {
        let spec = catalogue().get(&CheckId::parse(id).unwrap()).unwrap();
        let row = unesc(row);
        assert!(
            row.starts_with(&format!(r#" data-tone="{}""#, tones[i % 3])),
            "{id}: {row}"
        );
        assert!(
            row.contains(&format!(">{}</h3>", spec.title)),
            "{id}: {row}"
        );
        assert!(
            row.contains(&format!(r#"href="{}""#, spec.guide)),
            "{id}: {row}"
        );
        if i % 3 == 0 {
            assert!(!row.contains("check-row__fix\""), "{id}: {row}");
        } else {
            assert!(row.contains(&spec.fix_hint), "{id}: {row}");
            assert!(row.contains(&spec.finding), "{id}: {row}");
        }
    }
}

#[tokio::test]
async fn c8_heading_counts_lines_needing_attention_or_says_all_pass() {
    let tls = ids(Protocol::Tls);
    let mixed = tls
        .iter()
        .enumerate()
        .map(|(i, id)| match i {
            0 => result(id, Status::Fail, &["f"]),
            1 | 2 => result(id, Status::Warn, &["w"]),
            _ => result(id, Status::Pass, &[]),
        })
        .collect();
    let band = stub_band(Protocol::Tls, "tls", "h=example.com", mixed).await;
    let section = checks_section(&band);
    assert_eq!(
        text_between(section, r#"<h2 class="inspector-checks__title">"#, "</h2>"),
        "3 lines need attention"
    );
    assert_eq!(
        text_between(
            section,
            r#"<span class="inspector-checks__meta">"#,
            "</span>"
        ),
        "1 fail · 2 warn"
    );

    let pass = tls.iter().map(|id| result(id, Status::Pass, &[])).collect();
    let band = stub_band(Protocol::Tls, "tls", "h=example.com", pass).await;
    let section = checks_section(&band);
    assert_eq!(
        text_between(section, r#"<h2 class="inspector-checks__title">"#, "</h2>"),
        format!("All {} lines pass", tls.len())
    );
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

#[tokio::test]
async fn c9_dns_list_is_the_dns_checks_of_the_registrable_domain() {
    let sse = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/contracts/prism.sse"),
    )
    .unwrap();
    let domains = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder {
        inner: netray_dns::testing::golden_module(&sse),
        domains: domains.clone(),
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

    assert_eq!(*domains.lock().unwrap(), vec!["example.com".to_owned()]);
    let band = band_html(&events);
    let section = checks_section(&band);
    let want: Vec<String> = ids(Protocol::Dns)
        .iter()
        .map(|id| {
            catalogue()
                .get(&CheckId::parse(id).unwrap())
                .unwrap()
                .title
                .clone()
        })
        .collect();
    let got: Vec<String> = rows(section)
        .iter()
        .map(|r| unesc(text_between(r, r#"<h3 class="check-row__title">"#, "</h3>")))
        .collect();
    assert_eq!(got, want, "{section}");
}
