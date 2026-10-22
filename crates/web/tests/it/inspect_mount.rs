//! The inspector routes on `lens::app` (spec v2-inspectors, requirement 13): the real
//! `InspectorJobs` and v1's `IpExtractor` are mounted ahead of `spa::handler`.
//! Copied from crates/lens/tests/inspect_mount.rs (spec v2-cutover, Phase 6); dropped as V1: the
//! SPA half (the shell a client route answered; there is no SPA).
use crate::common;

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use netray_engine::Registry;
use netray_ip::lookup::{IpLookup, IpRefusal};
use netray_ip::testing::StubIpLookup;
use netray_web::admission::ApexState;
use netray_web::api::{AdmissionLimits, ApiConfig};
use netray_web::config::ApexConfig;
use tower::ServiceExt;

/// lens's `base_config`: the cache off, 60 per minute and a burst of 10 per IP, 1000 and 100
/// globally.
fn base_config() -> ApexConfig {
    let defaults = ApexConfig::default();
    ApexConfig {
        cache_enabled: false,
        cache_window_seconds: 300,
        limits: AdmissionLimits {
            rate_limit_per_ip_per_minute: 60,
            rate_limit_per_ip_burst: 10,
            rate_limit_global_per_minute: 1000,
            rate_limit_global_burst: 100,
            ..defaults.limits
        },
        ..defaults
    }
}

fn state(config: ApexConfig) -> ApexState {
    let registry = Registry::new();
    let plan = common::default_plan(&registry);
    common::state_with(config, registry, plan)
}

/// Records the address every caller lookup was made for, answers like the stub.
#[derive(Default)]
struct Recording {
    callers: Mutex<Vec<IpAddr>>,
    inner: StubIpLookup,
}

impl IpLookup for Recording {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<netray_ip::backend::Ifconfig, IpRefusal>> {
        self.inner.lookup(ip)
    }

    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
    ) -> BoxFuture<'a, Result<netray_ip::backend::Ifconfig, IpRefusal>> {
        self.callers.lock().unwrap().push(remote.ip());
        self.inner.lookup_caller(remote, user_agent)
    }

    fn range<'a>(
        &'a self,
        cidr: &'a str,
    ) -> BoxFuture<'a, Result<netray_ip::lookup::RangeResponse, IpRefusal>> {
        self.inner.range(cidr)
    }

    fn asn(
        &self,
        n: u32,
    ) -> BoxFuture<'_, Result<netray_ip::lookup::AsnLookupResponse, IpRefusal>> {
        self.inner.asn(n)
    }
}

async fn send(app: &Router, mut req: Request<Body>, peer: &str) -> (StatusCode, String) {
    let peer: SocketAddr = format!("{peer}:40000").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn post_empty_ip() -> Request<Body> {
    Request::post("/inspect/ip/run")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("x-forwarded-for", "192.0.2.7")
        .body(Body::from("ip="))
        .unwrap()
}

#[tokio::test]
async fn inspector_routes_are_mounted() {
    for api_enabled in [true, false] {
        let mut config = base_config();
        config.api = ApiConfig {
            enabled: api_enabled,
            ..Default::default()
        };
        let app = netray_web::host::apex::app(state(config));

        let (status, body) = send(&app, get("/inspect/tls"), "192.0.2.1").await;
        assert_eq!(status, StatusCode::OK, "api enabled={api_enabled}");
        assert!(
            body.contains(r#"data-stream-url="/inspect/tls/run""#),
            "api enabled={api_enabled}: {body}"
        );
        assert!(
            !body.contains("<div id=\"root\""),
            "api enabled={api_enabled}"
        );
    }
}

#[tokio::test]
async fn real_jobs_answer_unavailable_without_an_ip_part() {
    let mut state = state(base_config());
    state.api_parts.ip = None;
    let app = netray_web::host::apex::app(state);

    let (status, body) = send(&app, post_empty_ip(), "192.0.2.1").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("IP lookups are unavailable"), "body: {body}");
}

#[tokio::test]
async fn empty_ip_input_looks_up_the_caller_v1s_extractor_derives() {
    let mut config = base_config();
    config.trusted_proxies = vec!["203.0.113.9".to_string()];
    let lookup = Arc::new(Recording::default());
    let mut state = state(config);
    state.api_parts.ip = Some(lookup.clone());
    let app = netray_web::host::apex::app(state);

    let (status, body) = send(&app, post_empty_ip(), "203.0.113.9").await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        *lookup.callers.lock().unwrap(),
        vec!["192.0.2.7".parse::<IpAddr>().unwrap()],
        "body: {body}"
    );
}
