//! V2 spec v2-ip-ui, requirement 4 (C4, C16): a posted address looks up that address with reverse
//! DNS and renders the card without caller-only parts; an empty input looks up the caller with
//! reverse DNS; a run of a given address links back to the caller's own address.
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

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
            .push(format!("lookup_caller {}", remote.ip()));
        self.stub.lookup_caller(remote, user_agent)
    }
    fn lookup_with(
        &self,
        ip: IpAddr,
        options: LookupOptions,
    ) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        self.calls.lock().unwrap().push(format!(
            "lookup_with {ip} reverse_dns {}",
            options.reverse_dns
        ));
        self.stub.lookup_with(ip, options)
    }
    fn lookup_caller_with<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
        options: LookupOptions,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        self.calls.lock().unwrap().push(format!(
            "lookup_caller_with {} reverse_dns {}",
            remote.ip(),
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

fn app(ip: Option<Arc<Recording>>) -> Router {
    let config: ApiConfig = toml::from_str("[ip]\ninternal_mode = true\n").unwrap();
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

/// (event name, id, html) per event, posted as the trusted proxy forwarding for 192.0.2.7.
async fn post(app: &Router, body: &str) -> Vec<(String, String, String)> {
    let mut req = Request::post("/inspect/ip/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", "192.0.2.7")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut events = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let mut name = String::new();
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim().to_owned();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let (id, html) = match serde_json::from_str::<serde_json::Value>(&data) {
            Ok(j) => (
                j["id"].as_str().unwrap_or_default().to_owned(),
                j["html"].as_str().unwrap_or_default().to_owned(),
            ),
            Err(_) => (String::new(), String::new()),
        };
        events.push((name, id, html));
    }
    events
}

fn joined(events: &[(String, String, String)]) -> String {
    events.iter().map(|e| e.2.as_str()).collect()
}

#[tokio::test]
async fn posted_address_or_empty_input_looks_up_with_reverse_dns_and_links_back() {
    // C16: an explicit address
    let rec = Arc::new(Recording::default());
    let events = post(&app(Some(rec.clone())), "ip=198.51.100.1").await;
    assert_eq!(
        rec.calls.lock().unwrap().as_slice(),
        ["lookup_with 198.51.100.1 reverse_dns true"]
    );
    assert_eq!(events.last().unwrap().0, "end");
    let html = joined(&events);
    assert!(html.contains(r#"id="ev-ip.lookup""#), "{html}");
    assert!(html.contains("stub.example.com"), "{html}");
    assert!(html.contains(r#"href="/inspect/ip">Show my address</a>"#), "{html}");
    for absent in ["TCP source port", "User agent", "Request headers"] {
        assert!(!html.contains(absent), "{absent}: {html}");
    }

    // C4: empty input is the caller, with reverse DNS and no way back to itself
    let rec = Arc::new(Recording::default());
    let events = post(&app(Some(rec.clone())), "ip=").await;
    assert_eq!(
        rec.calls.lock().unwrap().as_slice(),
        ["lookup_caller_with 192.0.2.7 reverse_dns true"]
    );
    let html = joined(&events);
    assert!(!html.contains("Show my address"), "{html}");
}
