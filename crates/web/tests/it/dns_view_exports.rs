//! V2 spec v2-dns-ui, requirements 16 and 17 (C1 markup half, C2, C7): the DNS shell carries the
//! parse endpoint and the hint line (no other inspector does), and a finished DNS query's
//! exports name the `dns.answers` and `dns.servers` blocks.
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

const PROXY: &str = "203.0.113.9";
const CALLER: &str = "198.51.100.1";

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        Ok(Job::new(vec![]))
    }
}

async fn page(uri: &str) -> String {
    let app: Router = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let res = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "GET {uri}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The opening tag of the shell root.
fn root_tag(body: &str) -> &str {
    body.split("<div class=\"inspector-shell\"")
        .nth(1)
        .and_then(|s| s.split('>').next())
        .unwrap_or_default()
}

const HINT: &str = r#"<p class="inspector-shell__hint" id="inspector-parse-hint" hidden></p>"#;

#[tokio::test]
async fn only_the_dns_shell_carries_the_parse_endpoint_and_the_hint_line() {
    let dns = page("/inspect/dns").await;
    assert!(
        root_tag(&dns).contains(r#" data-suggest-parse="/api/v1/dns/parse""#),
        "{dns}"
    );
    assert!(dns.contains(HINT), "{dns}");

    let tls = page("/inspect/tls").await;
    assert!(!tls.contains("data-suggest-parse"), "{tls}");
    assert!(!tls.contains("inspector-parse-hint"), "{tls}");
}

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

fn dns_app() -> Router {
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

/// The `inspect-export` fragment html of a run of `q`.
async fn export_fragment(q: &str) -> String {
    let mut req = Request::post("/inspect/dns/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", CALLER)
        .body(Body::from(format!("q={}", q.replace(' ', "+"))))
        .unwrap();
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = dns_app().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let raw = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    for event in raw.split("\n\n") {
        let data: String = event
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect();
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        if j["id"] == "inspect-export" {
            return j["html"].as_str().unwrap_or_default().to_owned();
        }
    }
    panic!("no inspect-export fragment in {raw}");
}

fn unesc(s: &str) -> String {
    s.replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#x2f;", "/")
        .replace("&amp;", "&")
        .replace("&#38;", "&")
}

/// The raw inner text of each `<template data-export=...>`, by kind.
fn templates(html: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut rest = html;
    let open = "<template data-export=\"";
    while let Some(i) = rest.find(open) {
        let after = &rest[i + open.len()..];
        let kind = &after[..after.find('"').unwrap()];
        let start = after.find('>').unwrap() + 1;
        let end = after.find("</template>").expect("closing template");
        out.insert(kind.to_owned(), after[start..end].to_owned());
        rest = &after[end..];
    }
    out
}

#[tokio::test]
async fn a_finished_dns_query_exports_its_answers_and_servers_blocks() {
    let html = export_fragment("example.com A MX").await;
    let t = templates(&html);

    let json = unesc(&t["json"]);
    assert!(json.contains("dns.answers"), "{json}");
    assert!(json.contains("dns.servers"), "{json}");

    let csv = unesc(&t["csv"]);
    assert!(
        csv.lines().skip(1).any(|l| l.starts_with("dns.servers,")),
        "{csv}"
    );

    let md = unesc(&t["markdown"]);
    // Markdown names its sections by block title.
    assert!(md.contains("## Answers"), "{md}");
    assert!(md.contains("## Servers"), "{md}");
}
