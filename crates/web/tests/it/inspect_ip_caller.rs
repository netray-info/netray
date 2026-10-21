//! V2 spec v2-ip-ui, requirement 3 (C3, C11-C14): `GET /inspect/ip` without `ip` and `r` renders the
//! caller's run server-side, `Cache-Control: private, no-store`, refusals as a notice under the
//! address as h1. Driven through `inspect_router` over a recording `IpLookup` over `StubIpLookup`.
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_ip::backend::Ifconfig;
use netray_ip::lookup::LookupOptions;
use netray_ip::testing::StubIpLookup;
use netray_ip::{IpLookup, IpRefusal};
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

#[derive(Default)]
struct Recording {
    stub: StubIpLookup,
    calls: Mutex<Vec<String>>,
}

impl IpLookup for Recording {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        self.calls.lock().unwrap().push(format!("lookup {ip}"));
        self.stub.lookup(ip)
    }
    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("lookup_caller {remote}"));
        self.stub.lookup_caller(remote, user_agent)
    }
    fn lookup_with(
        &self,
        ip: IpAddr,
        options: LookupOptions,
    ) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        self.calls.lock().unwrap().push(format!("lookup_with {ip}"));
        self.stub.lookup_with(ip, options)
    }
    fn lookup_caller_with<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
        options: LookupOptions,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        self.calls.lock().unwrap().push(format!(
            "lookup_caller_with {remote} ua={} reverse_dns={}",
            user_agent.unwrap_or("-"),
            options.reverse_dns
        ));
        self.stub.lookup_caller_with(remote, user_agent, options)
    }
    fn range<'a>(
        &'a self,
        cidr: &'a str,
    ) -> BoxFuture<'a, Result<netray_ip::lookup::RangeResponse, IpRefusal>> {
        self.stub.range(cidr)
    }
    fn asn(
        &self,
        n: u32,
    ) -> BoxFuture<'_, Result<netray_ip::lookup::AsnLookupResponse, IpRefusal>> {
        self.stub.asn(n)
    }
}

const PROXY: &str = "203.0.113.9";
const PEER: &str = "192.0.2.10:54321";
const UA: &str = "caller-test/1.0";

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

fn app(ip: Option<Arc<Recording>>, api_toml: &str) -> Router {
    let config: ApiConfig = toml::from_str(api_toml).unwrap();
    let extractor = Arc::new(IpExtractor::new(&[PROXY.to_string()]));
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(ApiParts {
            ip: ip.map(|r| r as Arc<dyn IpLookup>),
            ..Default::default()
        });
    inspect_router(
        InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default())
            .with_ip_extractor(extractor),
    )
}

async fn get_caller(app: &Router) -> Reply {
    let mut req = Request::get("/inspect/ip")
        .header(header::USER_AGENT, UA)
        .body(Body::empty())
        .unwrap();
    let peer: SocketAddr = PEER.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

const H1: &str = r#"<h1 class="inspector-run__target">192.0.2.10</h1>"#;
const BASE: &str = "[ip]\ninternal_mode = true\n";

/// C11
#[tokio::test]
async fn get_without_ip_renders_the_callers_run_with_private_no_store() {
    let rec = Arc::new(Recording::default());
    let r = get_caller(&app(Some(rec), BASE)).await;

    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(r.body.contains(H1), "{}", r.body);
    assert!(r.body.contains(r#"id="ev-ip.lookup""#), "{}", r.body);
    assert!(r.body.contains("stub.example.com"), "{}", r.body);
    assert!(r.body.contains(r#"id="inspect-band""#), "{}", r.body);
    assert!(
        r.body.contains(r#"data-code="ip.reputation""#),
        "{}",
        r.body
    );
    assert!(!r.body.contains("data-share-key"), "{}", r.body);
    assert_eq!(r.headers[header::CACHE_CONTROL], "private, no-store");
}

/// C12
#[tokio::test]
async fn get_without_ip_calls_lookup_caller_with_once_with_port_agent_and_reverse_dns() {
    let rec = Arc::new(Recording::default());
    let r = get_caller(&app(Some(rec.clone()), BASE)).await;

    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        rec.calls.lock().unwrap().as_slice(),
        [format!(
            "lookup_caller_with {PEER} ua={UA} reverse_dns=true"
        )]
    );
}

/// C13
#[tokio::test]
async fn get_with_the_callers_token_spent_shows_the_refusal_under_the_address() {
    let rec = Arc::new(Recording::default());
    let app = app(
        Some(rec),
        "[ip]\ninternal_mode = true\nper_ip_burst = 1\nper_ip_per_minute = 1\n",
    );
    let first = get_caller(&app).await;
    assert!(first.body.contains(H1), "{}", first.body);

    let r = get_caller(&app).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(r.body.contains(H1), "{}", r.body);
    assert!(
        r.body.contains("too many requests; try again in"),
        "{}",
        r.body
    );
}

/// C14
#[tokio::test]
async fn get_without_an_ip_part_shows_unavailable_under_the_address() {
    let r = get_caller(&app(None, BASE)).await;

    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(r.body.contains(H1), "{}", r.body);
    assert!(r.body.contains("IP lookups are unavailable"), "{}", r.body);
}
