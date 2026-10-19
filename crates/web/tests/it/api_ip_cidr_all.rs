//! spec v2-ip-cli, criteria C7 C8 C28 C29 C30 C31 C32 C33: the caller fields `cidr` and `all`
//! on `/ip` and `/ip/{address}`, and the caller fields that answer with the IP module off.
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::extractors::{extract_headers, filter_headers, header_filters};
use netray_ip::handlers;
use netray_ip::testing::StubIpLookup;
use netray_ip::{IpLookup, LookupOptions};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger,
};
use serde_json::Value;
use tower::ServiceExt;

const CALLER: &str = "192.0.2.10:40000";
const UA: &str = "curl/8.5.0";

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: std::net::IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn app(config: ApiConfig, module_on: bool) -> Router {
    let parts = if module_on {
        ApiParts {
            ip: Some(Arc::new(StubIpLookup::new())),
            ..Default::default()
        }
    } else {
        ApiParts::default()
    };
    netray_web::api::router(ApiState::new(config, Arc::new(NoRuns)).with_parts(parts))
}

struct Reply {
    status: StatusCode,
    content_type: String,
    body: String,
}

fn request_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::USER_AGENT, HeaderValue::from_static(UA));
    h.insert(
        HeaderName::from_static("x-test"),
        HeaderValue::from_static("one"),
    );
    h
}

async fn get_with(app: Router, path: &str) -> Reply {
    let mut req = Request::get(path).body(Body::empty()).unwrap();
    *req.headers_mut() = request_headers();
    req.extensions_mut()
        .insert(ConnectInfo(CALLER.parse::<SocketAddr>().unwrap()));
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        content_type,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

async fn get(path: &str) -> Reply {
    get_with(app(ApiConfig::default(), true), path).await
}

async fn get_off(path: &str) -> Reply {
    get_with(app(ApiConfig::default(), false), path).await
}

fn assert_plain(r: &Reply, expected: &str, label: &str) {
    assert_eq!(r.status, StatusCode::OK, "{label}: {}", r.body);
    assert!(
        r.content_type.starts_with("text/plain"),
        "{label}: content type {}",
        r.content_type
    );
    assert_eq!(r.body, expected, "{label}");
}

fn default_filtered_headers() -> Vec<(String, String)> {
    let filters = header_filters(&ApiConfig::default().ip.filtered_headers);
    filter_headers(extract_headers(&request_headers()), &filters)
}

#[tokio::test]
async fn cidr_for_an_ipv4_caller_is_the_address_with_slash_32() {
    let r = get("/ip?field=cidr").await;
    assert_plain(&r, "192.0.2.10/32\n", "caller cidr");
}

#[tokio::test]
async fn cidr_for_an_ipv4_address_is_slash_32_and_for_an_ipv6_address_slash_128() {
    assert_plain(
        &get("/ip/198.51.100.1?field=cidr").await,
        "198.51.100.1/32\n",
        "v4",
    );
    assert_plain(
        &get("/ip/2001:db8::1?field=cidr").await,
        "2001:db8::1/128\n",
        "v6",
    );
}

#[tokio::test]
async fn cidr_stays_plain_text_with_format_json() {
    let r = get("/ip?field=cidr&format=json").await;
    assert_plain(&r, "192.0.2.10/32\n", "cidr json");
}

#[tokio::test]
async fn cidr_for_an_ipv6_address_in_internal_mode_is_slash_128() {
    let mut config = ApiConfig::default();
    config.ip.internal_mode = true;
    let r = get_with(app(config, true), "/ip/2001:db8::1?field=cidr").await;
    assert_plain(&r, "2001:db8::1/128\n", "internal mode cidr");
}

#[tokio::test]
async fn all_field_plain_is_the_all_rendering_followed_by_the_filtered_headers() {
    let caller = StubIpLookup::new()
        .lookup_caller_with(
            CALLER.parse().unwrap(),
            Some(UA),
            LookupOptions {
                reverse_dns: true,
                lang: None,
            },
        )
        .await
        .unwrap();
    let expected = format!(
        "{}{}",
        handlers::all::to_plain(&caller),
        handlers::headers::to_plain(&default_filtered_headers())
    );
    let r = get("/ip?field=all").await;
    assert_plain(&r, &expected, "all plain");
    assert!(r.body.lines().any(|l| l == "x-test: one"), "{}", r.body);
}

#[tokio::test]
async fn all_field_with_format_json_carries_a_top_level_headers_object() {
    let r = get("/ip?field=all&format=json").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let json: Value = serde_json::from_str(&r.body).expect("json body");
    assert_eq!(json["headers"]["x-test"], "one", "{}", r.body);
    assert_eq!(json["ip"]["addr"], "192.0.2.10", "{}", r.body);
    assert_eq!(
        json["headers"],
        handlers::headers::to_json_value(&default_filtered_headers()),
        "{}",
        r.body
    );
}

#[tokio::test]
async fn module_off_answers_headers_cidr_and_ip_for_the_caller() {
    assert_plain(&get_off("/ip?field=cidr").await, "192.0.2.10/32\n", "cidr");
    assert_plain(&get_off("/ip?field=ip").await, "192.0.2.10\n", "ip");
    assert_plain(
        &get_off("/ip?field=headers").await,
        &handlers::headers::to_plain(&default_filtered_headers()),
        "headers",
    );
}

#[tokio::test]
async fn module_off_keeps_every_lookup_needing_field_at_503_unavailable() {
    for name in [
        "country",
        "asn",
        "city",
        "region",
        "timezone",
        "latitude",
        "longitude",
        "isp",
        "location",
        "network",
        "all",
    ] {
        let r = get_off(&format!("/ip?field={name}")).await;
        assert_eq!(
            r.status,
            StatusCode::SERVICE_UNAVAILABLE,
            "{name}: {}",
            r.body
        );
        let v: Value = serde_json::from_str(&r.body).unwrap_or_else(|_| panic!("{name}: not JSON"));
        assert_eq!(v["error"]["code"], "UNAVAILABLE", "{name}: {}", r.body);
    }
}
