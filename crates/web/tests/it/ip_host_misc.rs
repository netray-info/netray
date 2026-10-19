//! spec v2-ip-cli Phase 3, criteria C3 C23 C24 C25: `ip_host` answers `?ip=X` on every path,
//! keeps plain root, `/ip/cidr` and `/headers` alive with the IP module off, refuses `/tcp?ip=`
//! naming the field, and carries the rate-limit, GeoIP and ETag headers.
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::extractors::{extract_headers, filter_headers, header_filters};
use netray_ip::handlers;
use netray_ip::testing::StubIpLookup;
use netray_ip::{IpLookup, LookupOptions};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, ip_host,
};
use serde_json::Value;

const CALLER: &str = "192.0.2.10:40000";
const UA: &str = "curl/8.5.0";
const TARGET: &str = "198.51.100.1";

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

fn state(config: ApiConfig, module_on: bool) -> ApiState {
    let parts = if module_on {
        ApiParts {
            ip: Some(Arc::new(StubIpLookup::new())),
            ..Default::default()
        }
    } else {
        ApiParts::default()
    };
    ApiState::new(config, Arc::new(NoRuns)).with_parts(parts)
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
}

async fn call(state: &ApiState, path: &str, extra: &[(&str, &str)]) -> Reply {
    let mut b = Request::get(path).header("user-agent", UA);
    for (n, v) in extra {
        b = b.header(*n, *v);
    }
    let mut req = b.body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(CALLER.parse::<SocketAddr>().unwrap()));
    let (method, uri, headers) = (
        req.method().clone(),
        req.uri().clone(),
        req.headers().clone(),
    );
    match ip_host(state, req).await {
        Ok(resp) => {
            let status = resp.status();
            let headers = resp.headers().clone();
            let bytes = resp.into_body().collect().await.unwrap().to_bytes();
            Reply {
                status,
                headers,
                body: String::from_utf8(bytes.to_vec()).unwrap(),
            }
        }
        Err(back) => {
            assert_eq!(back.method(), &method);
            assert_eq!(back.uri(), &uri);
            assert_eq!(back.headers(), &headers);
            panic!("ip_host declined {path}");
        }
    }
}

async fn target() -> netray_ip::backend::Ifconfig {
    StubIpLookup::new()
        .lookup_with(
            TARGET.parse().unwrap(),
            LookupOptions {
                reverse_dns: true,
                lang: None,
            },
        )
        .await
        .unwrap()
}

fn json(r: &Reply) -> Value {
    serde_json::from_str(&r.body).unwrap_or_else(|e| panic!("{e}: {}", r.body))
}

#[tokio::test]
async fn c3_every_ip_host_path_answers_for_the_ip_query() {
    let st = state(ApiConfig::default(), true);
    let ifc = target().await;

    let r = call(&st, &format!("/?ip={TARGET}"), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body, format!("{TARGET}\n"));

    let r = call(&st, &format!("/json?ip={TARGET}"), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(json(&r)["ip"]["addr"], TARGET);

    let r = call(&st, &format!("/country?ip={TARGET}"), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body, handlers::country::to_plain(&ifc));

    let r = call(&st, &format!("/ip/cidr?ip={TARGET}"), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body, "198.51.100.1/32\n");

    let r = call(&st, &format!("/all?ip={TARGET}"), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.body.starts_with(&handlers::all::to_plain(&ifc)),
        "{}",
        r.body
    );

    let r = call(&st, &format!("/headers?ip={TARGET}"), &[("x-test", "one")]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let mut req_headers = axum::http::HeaderMap::new();
    req_headers.insert("user-agent", UA.parse().unwrap());
    req_headers.insert("x-test", "one".parse().unwrap());
    let filters = header_filters(&ApiConfig::default().ip.filtered_headers);
    let expected =
        handlers::headers::to_plain(&filter_headers(extract_headers(&req_headers), &filters));
    assert_eq!(r.body, expected);
    assert!(r.body.contains("x-test: one"), "{}", r.body);
}

#[tokio::test]
async fn c3_caller_only_paths_with_an_ip_query_are_400_naming_the_field() {
    let st = state(ApiConfig::default(), true);
    for field in ["tcp", "user_agent"] {
        let r = call(&st, &format!("/{field}?ip={TARGET}"), &[]).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{field}: {}", r.body);
        let v = json(&r);
        assert_eq!(v["error"]["code"], "INVALID_INPUT", "{field}");
        assert!(
            v["error"]["message"].as_str().unwrap().contains(field),
            "{field}: {}",
            r.body
        );
    }
}

#[tokio::test]
async fn c3_dns_false_and_fields_apply_on_the_json_path() {
    let st = state(ApiConfig::default(), true);
    let r = call(&st, &format!("/json?ip={TARGET}&dns=false"), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(json(&r)["ip"]["hostname"].is_null(), "{}", r.body);

    let r = call(&st, &format!("/json?ip={TARGET}&fields=ip"), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let v = json(&r);
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["ip"], "{}", r.body);
}

#[tokio::test]
async fn c3_the_per_ip_limiter_is_shared_with_the_api_and_the_third_country_is_429() {
    let mut config = ApiConfig::default();
    config.ip.per_ip_burst = 2;
    let st = state(config, true);
    for n in 0..2 {
        let r = call(&st, "/country", &[]).await;
        assert_eq!(r.status, StatusCode::OK, "request {n}: {}", r.body);
    }
    let r = call(&st, "/country", &[]).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.body);
}

#[tokio::test]
async fn c23_module_off_keeps_root_cidr_and_headers_and_refuses_country() {
    let st = state(ApiConfig::default(), false);
    let r = call(&st, "/", &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body, "192.0.2.10\n");
    let r = call(&st, "/ip/cidr", &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let r = call(&st, "/headers", &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let r = call(&st, "/country", &[]).await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE, "{}", r.body);
    assert_eq!(json(&r)["error"]["code"], "UNAVAILABLE");
}

#[tokio::test]
async fn c24_tcp_with_an_ip_query_is_400_naming_tcp() {
    let st = state(ApiConfig::default(), true);
    let r = call(&st, "/tcp?ip=198.51.100.1", &[]).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.body);
    assert!(
        json(&r)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("tcp"),
        "{}",
        r.body
    );
}

#[tokio::test]
async fn c25_country_carries_rate_limit_geoip_and_a_strong_etag_that_revalidates() {
    let st = state(ApiConfig::default(), true);
    let r = call(&st, "/country", &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(r.header("x-ratelimit-limit").is_some(), "{:?}", r.headers);
    assert!(
        r.header("x-geoip-database-date").is_some(),
        "{:?}",
        r.headers
    );
    assert!(
        r.header("x-geoip-database-age-days").is_some(),
        "{:?}",
        r.headers
    );
    let etag = r.header("etag").expect("etag").to_string();
    assert!(etag.starts_with('"'), "strong etag, got {etag}");

    let again = call(&st, "/country", &[("if-none-match", &etag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
}
