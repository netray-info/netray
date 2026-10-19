//! API v1 response metadata headers on the ip routes (spec v2-ip-cli, Phase 2, requirements 10 and
//! 11; criteria C1-C10): rate-limit headers, GeoIP database date and age, strong ETag, Last-Modified
//! and conditional GET, driven through `netray_web::api::router` over `StubIpLookup`.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::lookup::{AsnLookupResponse, IpLookup, IpRefusal, RangeResponse};
use netray_ip::backend::Ifconfig;
use netray_ip::testing::{STUB_GEOIP_EPOCH, StubIpLookup};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger,
};
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
        unreachable!("no run is started by the IP routes")
    }
}

/// Delegates the required methods to the stub and has no GeoIP build epoch.
struct NoEpoch(StubIpLookup);

impl IpLookup for NoEpoch {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        self.0.lookup(ip)
    }
    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        self.0.lookup_caller(remote, user_agent)
    }
    fn range<'a>(&'a self, cidr: &'a str) -> BoxFuture<'a, Result<RangeResponse, IpRefusal>> {
        self.0.range(cidr)
    }
    fn asn(&self, n: u32) -> BoxFuture<'_, Result<AsnLookupResponse, IpRefusal>> {
        self.0.asn(n)
    }
}

fn app_with(ip: Arc<dyn IpLookup>) -> Router {
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(ApiParts {
        ip: Some(ip),
        ..Default::default()
    });
    netray_web::api::router(state)
}

fn app() -> Router {
    app_with(Arc::new(StubIpLookup::new()))
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

async fn send(app: &Router, mut req: Request<Body>) -> Reply {
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::new("192.0.2.10".parse().unwrap(), 40000)));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
    Reply { status, headers, body }
}

async fn get(app: &Router, path: &str, if_none_match: Option<&str>) -> Reply {
    let mut b = Request::get(path);
    if let Some(v) = if_none_match {
        b = b.header("if-none-match", v);
    }
    send(app, b.body(Body::empty()).unwrap()).await
}

fn h<'a>(r: &'a Reply, name: &str) -> Option<&'a str> {
    r.headers.get(name).map(|v| v.to_str().unwrap())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

/// The stub epoch as an HTTP date; the literal is valid for 1_700_000_000 only.
const STUB_HTTP_DATE: &str = "Tue, 14 Nov 2023 22:13:20 GMT";

#[tokio::test]
async fn first_get_ip_carries_limit_60_and_remaining_9() {
    // C1, C3
    let app = app();
    let r = get(&app, "/ip/198.51.100.1", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(h(&r, "x-ratelimit-limit"), Some("60"));
    assert_eq!(h(&r, "x-ratelimit-remaining"), Some("9"));
    let r = get(&app, "/ip", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(h(&r, "x-ratelimit-limit"), Some("60"));
    assert_eq!(h(&r, "x-ratelimit-remaining"), Some("8"));
}

#[tokio::test]
async fn past_the_burst_answers_429_with_remaining_0_and_retry_after() {
    // C1, C4
    let app = app();
    let burst = ApiConfig::default().ip.per_ip_burst;
    for _ in 0..burst {
        let r = get(&app, "/ip/198.51.100.1", None).await;
        assert_eq!(r.status, StatusCode::OK);
    }
    let r = get(&app, "/ip/198.51.100.1", None).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(h(&r, "x-ratelimit-remaining"), Some("0"));
    assert_eq!(h(&r, "x-ratelimit-limit"), Some("60"));
    let retry = h(&r, "retry-after").expect("retry-after");
    assert!(retry.parse::<u64>().unwrap() >= 1, "retry-after {retry}");
}

#[tokio::test]
async fn stub_epoch_gives_geoip_date_and_whole_day_age() {
    // C1, C5
    assert_eq!(STUB_GEOIP_EPOCH, 1_700_000_000, "STUB_HTTP_DATE is derived from this epoch");
    let app = app();
    let before = (now() - STUB_GEOIP_EPOCH) / 86400;
    let r = get(&app, "/ip/198.51.100.1", None).await;
    let after = (now() - STUB_GEOIP_EPOCH) / 86400;
    assert_eq!(h(&r, "x-geoip-database-date"), Some(STUB_HTTP_DATE));
    let age: u64 = h(&r, "x-geoip-database-age-days")
        .expect("age header")
        .parse()
        .expect("whole days");
    assert!((before..=after).contains(&age), "age {age} not in {before}..={after}");
}

#[tokio::test]
async fn seam_without_build_epoch_has_no_geoip_header_and_no_last_modified() {
    // C6
    let app = app_with(Arc::new(NoEpoch(StubIpLookup::new())));
    let r = get(&app, "/ip/198.51.100.1", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(h(&r, "x-ratelimit-limit"), Some("60"), "rate-limit headers stay");
    assert!(r.headers.get("x-geoip-database-date").is_none());
    assert!(r.headers.get("x-geoip-database-age-days").is_none());
    assert!(r.headers.get("last-modified").is_none());
}

#[tokio::test]
async fn two_gets_carry_the_same_strong_etag_and_last_modified_is_the_geoip_date() {
    // C2, C7
    let app = app();
    let a = get(&app, "/ip/198.51.100.1", None).await;
    let b = get(&app, "/ip/198.51.100.1", None).await;
    let etag = h(&a, "etag").expect("etag");
    assert!(etag.starts_with('"') && etag.ends_with('"'), "strong etag, got {etag}");
    assert!(!etag.starts_with("W/"));
    assert_eq!(h(&b, "etag"), Some(etag));
    assert_eq!(h(&a, "last-modified"), Some(STUB_HTTP_DATE));
    assert_eq!(h(&a, "last-modified"), h(&a, "x-geoip-database-date"));
    let other = get(&app, "/ip/198.51.100.2", None).await;
    assert_ne!(h(&other, "etag"), Some(etag), "etag derives from the body");
}

#[tokio::test]
async fn if_none_match_equal_or_star_gives_304_with_etag_and_empty_body() {
    // C2, C8
    let app = app();
    let first = get(&app, "/ip/198.51.100.1", None).await;
    let etag = h(&first, "etag").expect("etag").to_string();
    for inm in [etag.as_str(), "*"] {
        let r = get(&app, "/ip/198.51.100.1", Some(inm)).await;
        assert_eq!(r.status, StatusCode::NOT_MODIFIED, "If-None-Match {inm}");
        assert_eq!(h(&r, "etag"), Some(etag.as_str()));
        assert!(r.body.is_empty());
    }
}

#[tokio::test]
async fn different_if_none_match_gives_200_with_the_body() {
    // C9
    let app = app();
    let first = get(&app, "/ip/198.51.100.1", None).await;
    let r = get(&app, "/ip/198.51.100.1", Some("\"not-the-etag\"")).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(!r.body.is_empty());
    assert_eq!(r.body, first.body);
    assert!(h(&r, "etag").is_some());
}

#[tokio::test]
async fn diff_post_and_asn_get_carry_rate_limit_headers_and_the_post_no_etag() {
    // C10
    let app = app();
    let post = send(
        &app,
        Request::post("/ip/diff")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"a":"198.51.100.1","b":"198.51.100.2"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(post.status, StatusCode::OK);
    assert_eq!(h(&post, "x-ratelimit-limit"), Some("60"));
    assert_eq!(h(&post, "x-ratelimit-remaining"), Some("8"), "diff costs 2");
    assert!(post.headers.get("etag").is_none());
    let asn = get(&app, "/asn/15169", None).await;
    assert_eq!(asn.status, StatusCode::OK);
    assert_eq!(h(&asn, "x-ratelimit-limit"), Some("60"));
    assert_eq!(h(&asn, "x-ratelimit-remaining"), Some("7"));
}
