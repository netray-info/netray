//! spec v2-ip-cli Phase 3, criteria C2, C4, C5, C6, C7, C8: `ip_host` on `/` negotiates in V1 order
//! (`?format=` first; a CLI agent with `*/*` or no Accept gets plain; otherwise the first named
//! format in Accept decides; HTML or nothing is `Err(req)`).
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::format::OutputFormat;
use netray_ip::handlers;
use netray_ip::testing::StubIpLookup;
use netray_ip::{IpLookup, LookupOptions};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, ip_host,
};

const CALLER: &str = "192.0.2.10:40000";
const CURL: &str = "curl/8.5.0";
const BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64) Gecko/20100101 Firefox/130.0";
const BROWSER_ACCEPT: &str = "text/html,application/xhtml+xml,*/*;q=0.8";

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

fn opts() -> LookupOptions {
    LookupOptions {
        reverse_dns: true,
        lang: None,
    }
}

fn request(method: Method, uri: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut req = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let mut req = req.body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(CALLER.parse::<SocketAddr>().unwrap()));
    req
}

struct Reply {
    status: StatusCode,
    content_type: String,
    body: String,
}

async fn answer(uri: &str, headers: &[(&str, &str)]) -> Reply {
    answer_with(Method::GET, uri, headers).await
}

async fn answer_with(method: Method, uri: &str, headers: &[(&str, &str)]) -> Reply {
    let resp = ip_host(&state(), request(method, uri, headers))
        .await
        .unwrap_or_else(|_| panic!("{uri} {headers:?}: expected a response, got Err(req)"));
    let status = resp.status();
    let content_type = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        content_type,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

async fn assert_declined(uri: &str, headers: &[(&str, &str)]) {
    let sent = request(Method::GET, uri, headers);
    let (method, sent_uri, sent_headers) = (
        sent.method().clone(),
        sent.uri().clone(),
        sent.headers().clone(),
    );
    let back = match ip_host(&state(), sent).await {
        Ok(resp) => panic!(
            "{uri} {headers:?}: expected Err(req), got {}",
            resp.status()
        ),
        Err(req) => req,
    };
    assert_eq!(back.method(), &method, "{uri} method");
    assert_eq!(back.uri(), &sent_uri, "{uri} uri");
    assert_eq!(back.headers(), &sent_headers, "{uri} headers");
}

async fn caller_json(user_agent: Option<&str>) -> serde_json::Value {
    let ifc = StubIpLookup::new()
        .lookup_caller_with(CALLER.parse().unwrap(), user_agent, opts())
        .await
        .unwrap();
    handlers::root::to_json(&ifc).unwrap()
}

fn assert_plain(r: &Reply, body: &str, label: &str) {
    assert_eq!(r.status, StatusCode::OK, "{label}: {}", r.body);
    assert!(
        r.content_type.starts_with("text/plain; charset=utf-8"),
        "{label}: content type {}",
        r.content_type
    );
    assert_eq!(r.body, body, "{label}");
}

#[tokio::test]
async fn curl_with_accept_any_on_root_gets_the_caller_address_as_plain_text() {
    let r = answer("/", &[("user-agent", CURL), ("accept", "*/*")]).await;
    assert_plain(&r, "192.0.2.10\n", "curl */*");
}

#[tokio::test]
async fn curl_without_accept_on_root_gets_plain_text() {
    let r = answer("/", &[("user-agent", CURL)]).await;
    assert_plain(&r, "192.0.2.10\n", "curl no accept");
}

#[tokio::test]
async fn head_root_from_curl_answers_200() {
    let r = answer_with(Method::HEAD, "/", &[("user-agent", CURL)]).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(
        r.content_type.starts_with("text/plain"),
        "{}",
        r.content_type
    );
}

#[tokio::test]
async fn curl_on_root_with_ip_query_gets_that_address() {
    let r = answer("/?ip=198.51.100.1", &[("user-agent", CURL)]).await;
    assert_plain(&r, "198.51.100.1\n", "curl ?ip=");
}

#[tokio::test]
async fn a_non_cli_agent_naming_text_plain_gets_plain_text() {
    let r = answer("/", &[("user-agent", BROWSER), ("accept", "text/plain")]).await;
    assert_plain(&r, "192.0.2.10\n", "browser text/plain");
}

#[tokio::test]
async fn a_non_cli_agent_with_accept_any_on_root_is_declined() {
    assert_declined("/", &[("user-agent", BROWSER), ("accept", "*/*")]).await;
}

#[tokio::test]
async fn a_browser_asking_for_html_on_root_is_declined_unchanged() {
    assert_declined("/", &[("user-agent", BROWSER), ("accept", BROWSER_ACCEPT)]).await;
}

#[tokio::test]
async fn a_request_with_no_agent_and_no_accept_on_root_is_declined() {
    assert_declined("/", &[]).await;
}

#[tokio::test]
async fn curl_with_accept_json_on_root_gets_the_json_document() {
    let r = answer("/", &[("user-agent", CURL), ("accept", "application/json")]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("application/json"),
        "{}",
        r.content_type
    );
    let doc = caller_json(Some(CURL)).await;
    assert_eq!(r.body, OutputFormat::Json.serialize_body(&doc).unwrap());
}

#[tokio::test]
async fn the_first_named_format_in_accept_decides_on_root() {
    let doc = caller_json(Some(BROWSER)).await;
    for (accept, format, ct) in [
        (
            "application/yaml, application/json",
            OutputFormat::Yaml,
            "application/yaml",
        ),
        ("text/csv, application/json", OutputFormat::Csv, "text/csv"),
        (
            "application/toml;q=0.5, text/html",
            OutputFormat::Toml,
            "application/toml",
        ),
        (
            "*/*, application/json",
            OutputFormat::Json,
            "application/json",
        ),
    ] {
        let r = answer("/", &[("user-agent", BROWSER), ("accept", accept)]).await;
        assert_eq!(r.status, StatusCode::OK, "{accept}: {}", r.body);
        assert!(
            r.content_type.starts_with(ct),
            "{accept}: {}",
            r.content_type
        );
        assert_eq!(r.body, format.serialize_body(&doc).unwrap(), "{accept}");
    }
}

#[tokio::test]
async fn html_named_before_json_in_accept_declines_on_root() {
    assert_declined(
        "/",
        &[
            ("user-agent", BROWSER),
            ("accept", "text/html, application/json"),
        ],
    )
    .await;
}

#[tokio::test]
async fn format_query_on_root_beats_agent_and_accept() {
    let doc = caller_json(Some(BROWSER)).await;
    let expected = OutputFormat::Yaml.serialize_body(&doc).unwrap();
    for headers in [
        vec![("user-agent", BROWSER), ("accept", BROWSER_ACCEPT)],
        vec![("user-agent", BROWSER), ("accept", "application/json")],
        vec![("user-agent", BROWSER)],
        vec![],
    ] {
        let r = answer("/?format=yaml", &headers).await;
        assert_eq!(r.status, StatusCode::OK, "{headers:?}: {}", r.body);
        assert!(
            r.content_type.starts_with("application/yaml"),
            "{headers:?}: {}",
            r.content_type
        );
        let ua = headers
            .iter()
            .find(|(n, _)| *n == "user-agent")
            .map(|(_, v)| *v);
        let expected = if ua == Some(BROWSER) {
            expected.clone()
        } else {
            OutputFormat::Yaml
                .serialize_body(&caller_json(ua).await)
                .unwrap()
        };
        assert_eq!(r.body, expected, "{headers:?}");
    }
}
