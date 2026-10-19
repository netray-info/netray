//! spec v2-ip-cli, Phase 3 review: HEAD keeps GET's Content-Length, every answer is private and
//! varies on Accept and User-Agent, a field path with a trailing slash is handed back, and the
//! refusals ip_host answers itself carry the rate-limit header (requirements 10, 13, 15).
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use futures::future::BoxFuture;
use netray_ip::testing::StubIpLookup;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, ip_host,
};

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

fn state() -> ApiState {
    ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(ApiParts {
        ip: Some(Arc::new(StubIpLookup::new())),
        ..Default::default()
    })
}

async fn call(method: Method, path: &str) -> Result<Response, Request<Body>> {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("user-agent", "curl/8.5.0")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "192.0.2.10:40000".parse::<SocketAddr>().unwrap(),
    ));
    ip_host(&state(), req).await
}

fn header(resp: &Response, name: header::HeaderName) -> Option<String> {
    resp.headers()
        .get(name)
        .map(|v| v.to_str().unwrap().to_string())
}

#[tokio::test]
async fn head_carries_the_content_length_of_get() {
    let get = call(Method::GET, "/country").await.unwrap();
    let head = call(Method::HEAD, "/country").await.unwrap();
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(
        header(&head, header::CONTENT_LENGTH),
        Some("Germany\n".len().to_string())
    );
    assert_eq!(get.status(), StatusCode::OK);
}

#[tokio::test]
async fn answers_are_private_and_vary_on_accept_and_user_agent() {
    for path in ["/", "/country", "/json"] {
        let resp = call(Method::GET, path).await.unwrap();
        let cache = header(&resp, header::CACHE_CONTROL).unwrap_or_default();
        assert!(cache.contains("private"), "{path}: {cache}");
        let vary = header(&resp, header::VARY).unwrap_or_default();
        assert!(vary.contains("Accept"), "{path}: {vary}");
        assert!(vary.contains("User-Agent"), "{path}: {vary}");
    }
}

#[tokio::test]
async fn a_field_path_with_a_trailing_slash_is_handed_back() {
    for path in ["/country/", "/asn/", "/headers/"] {
        let back = call(Method::GET, path).await;
        assert!(back.is_err(), "{path} answered instead of handing back");
    }
}

#[tokio::test]
async fn ip_host_refusals_carry_the_rate_limit_header() {
    for path in ["/country?format=xml", "/country/xml"] {
        let resp = call(Method::GET, path).await.unwrap();
        assert!(resp.status().is_client_error(), "{path}");
        assert!(
            resp.headers().contains_key("x-ratelimit-limit"),
            "{path}: {:?}",
            resp.headers()
        );
    }
}
