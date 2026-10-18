//! V2 spec v2-inspectors, requirements 18, 21 (C1 route half, C6, C8, C10 job half): the DNS
//! inspector job parses `q` with prism's parser, runs one part per requested job, starts no job
//! for `+check`, answers a parse error in one fragment, and charges the `[api.dns]` limiter
//! that v1 charges. Driven through `inspect_router` over `InspectJobs` and the DNS stubs.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, Response, StatusCode, header};
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

fn apps(config: ApiConfig) -> (Router, Router) {
    let extractor = Arc::new(IpExtractor::new(&[PROXY.to_string()]));
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(ApiParts {
            dns: Some(netray_dns::testing::stub_jobs()),
            dns_module: Some(Arc::from(netray_dns::testing::golden_module(&golden()))),
            ..Default::default()
        });
    let v1 = netray_web::api::router(state.clone());
    let deps = InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default())
        .with_ip_extractor(extractor);
    (inspect_router(deps), v1)
}

fn peer(mut req: Request<Body>) -> Request<Body> {
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    req
}

async fn post_run(app: &Router, q: &str) -> Response<Body> {
    let body = format!("q={}", enc(q));
    let req = Request::post("/inspect/dns/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", CALLER)
        .body(Body::from(body))
        .unwrap();
    app.clone().oneshot(peer(req)).await.unwrap()
}

async fn text(resp: Response<Body>) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes)
        .replace("\r\n", "\n")
        .replace("\\\"", "\"")
}

fn has_part(stream: &str, id: &str, shape: &str) -> bool {
    stream.contains(&format!("id=\"ev-{id}\" data-shape=\"{shape}\""))
}

/// Form-encodes the flag `+` as `%2B`, spaces as `+`.
fn enc(q: &str) -> String {
    q.replace('+', "%2B").replace(' ', "+")
}

async fn run_q(app: &Router, q: &str) -> String {
    text(post_run(app, q).await).await
}

#[tokio::test]
async fn c6_trace_flag_streams_a_trace_tree_and_the_query_records() {
    let (app, _) = apps(ApiConfig::default());

    let s = run_q(&app, "example.com A +trace").await;

    assert!(has_part(&s, "dns.trace", "tree"), "{s}");
    assert!(s.contains("id=\"ev-dns.a\""), "{s}");
}

#[tokio::test]
async fn c8_compare_and_auth_flags_stream_their_tables() {
    let (app, _) = apps(ApiConfig::default());
    // (q, block id)
    for (q, id) in [
        ("example.com +compare", "dns.compare"),
        ("example.com +auth", "dns.authcompare"),
    ] {
        let s = run_q(&app, q).await;

        assert!(has_part(&s, id, "table"), "{q}: {s}");
    }
}

#[tokio::test]
async fn c10_check_flag_starts_no_job() {
    let (app, _) = apps(ApiConfig::default());

    let s = run_q(&app, "example.com +check").await;

    // One part event: the query; `+check` adds none.
    assert_eq!(s.matches(r#""id":"inspect-part-"#).count(), 1, "{s}");
    assert!(!s.contains("check pending"), "{s}");
    assert!(s.contains("id=\"ev-dns.a\""), "{s}");
}

#[tokio::test]
async fn a_parse_error_answers_one_fragment_with_the_message_then_end() {
    let (app, _) = apps(ApiConfig::default());

    let s = run_q(&app, "example.com ANY").await;

    let events: Vec<&str> = s.split("\n\n").filter(|e| !e.is_empty()).collect();
    assert_eq!(events.len(), 2, "{s}");
    assert!(events[0].starts_with("event: fragment"), "{s}");
    assert!(events[0].contains("blocked query type: ANY"), "{s}");
    assert!(events[1].starts_with("event: end"), "{s}");
}

#[tokio::test]
async fn the_inspector_charges_the_api_dns_limiter_v1_shares() {
    let config: ApiConfig = toml::from_str("[dns]\nper_ip_burst = 18\nper_ip_per_minute = 1")
        .expect("[api.dns] parses");
    let (app, v1) = apps(config);
    let req = Request::post("/dns/trace")
        .header("content-type", "application/json")
        .header("x-forwarded-for", CALLER)
        .body(Body::from(r#"{"domain":"example.com"}"#))
        .unwrap();
    let first = v1.oneshot(peer(req)).await.unwrap();
    assert_ne!(
        first.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "v1 trace must be admitted"
    );

    let resp = post_run(&app, "example.com +trace").await;

    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(resp.headers().contains_key("retry-after"));
    let s = text(resp).await;
    assert!(s.contains("too many requests"), "{s}");
}
