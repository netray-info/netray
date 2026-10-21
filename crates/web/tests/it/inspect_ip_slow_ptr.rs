//! V2 spec v2-ip-ui, requirements 3 and 4 (review blocker of phase 1): a reverse-DNS lookup that
//! never answers costs the host name, never the card. The lookup with reverse DNS has a budget
//! inside the IP inspector's deadline; past it the address is looked up without reverse DNS.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_ip::backend::Ifconfig;
use netray_ip::testing::StubIpLookup;
use netray_ip::{IpLookup, IpRefusal, LookupOptions};
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

/// A lookup whose reverse DNS never answers; without reverse DNS it answers at once.
#[derive(Default)]
struct HangingPtr {
    stub: StubIpLookup,
}

impl IpLookup for HangingPtr {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        self.stub.lookup(ip)
    }
    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        self.lookup_caller_with(remote, user_agent, LookupOptions::default())
    }
    fn lookup_with(
        &self,
        ip: IpAddr,
        options: LookupOptions,
    ) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        if options.reverse_dns {
            return Box::pin(futures::future::pending());
        }
        self.stub.lookup_with(ip, options)
    }
    fn lookup_caller_with<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
        options: LookupOptions,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        if options.reverse_dns {
            return Box::pin(futures::future::pending());
        }
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

fn app() -> Router {
    let config: ApiConfig = toml::from_str("[ip]\ninternal_mode = true\n").unwrap();
    let extractor = Arc::new(IpExtractor::new(&[]));
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(ApiParts {
            ip: Some(Arc::new(HangingPtr::default()) as Arc<dyn IpLookup>),
            ..Default::default()
        });
    // A short deadline keeps the test fast; the reverse lookup's budget is a share of it.
    let mut inspect = InspectConfig::default();
    inspect.ip.deadline_ms = 400;
    inspect_router(
        InspectDeps::new(Arc::new(InspectJobs::new(state)), inspect).with_ip_extractor(extractor),
    )
}

async fn body(app: &Router, req: Request<Body>) -> String {
    let mut req = req;
    let peer: SocketAddr = "192.0.2.10:54321".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn a_hanging_reverse_lookup_costs_the_host_name_not_the_posted_card() {
    let req = Request::post("/inspect/ip/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("ip=198.51.100.1"))
        .unwrap();
    let text = body(&app(), req).await;
    assert!(text.contains(r#"id=\"ev-ip.lookup\""#), "{text}");
    assert!(text.contains("no reverse DNS"), "{text}");
    assert!(!text.contains("timed out"), "{text}");
}

#[tokio::test]
async fn a_hanging_reverse_lookup_costs_the_host_name_not_the_callers_card() {
    let req = Request::get("/inspect/ip").body(Body::empty()).unwrap();
    let text = body(&app(), req).await;
    assert!(text.contains(r#"id="ev-ip.lookup""#), "{text}");
    assert!(text.contains("no reverse DNS"), "{text}");
    assert!(!text.contains("timed out"), "{text}");
}
