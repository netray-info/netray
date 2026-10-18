//! C15, C6 (requirement 14): the inspector pages and API v1 draw on one set of limiters. A budget
//! spent through v1 refuses the page request (429, `Retry-After`, one "too many requests"
//! fragment) and the page request starts no inspection. Both routers are built over one `ApiState`.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_http::testing::StubHttpInspect;
use netray_ip::testing::StubIpLookup;
use netray_tls::testing::{StubTlsBackend, stub_state};
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
        unreachable!("no run is started by the inspector routes")
    }
}

const PROXY: &str = "203.0.113.9";

/// v1 router and inspector router over one `ApiState`.
fn setup(config: ApiConfig, parts: ApiParts) -> (Router, Router) {
    let extractor = Arc::new(IpExtractor::new(&[PROXY.to_string()]));
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(parts);
    let v1 = netray_web::api::router(state.clone());
    let pages = inspect_router(
        InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default())
            .with_ip_extractor(extractor),
    );
    (v1, pages)
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

/// Sends `req` as the trusted proxy forwarding for `caller`.
async fn send(app: &Router, mut req: Request<Body>) -> Reply {
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
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

async fn v1_post(app: &Router, caller: &str, path: &str, json: &str) -> Reply {
    let req = Request::post(path)
        .header("content-type", "application/json")
        .header("x-forwarded-for", caller)
        .body(Body::from(json.to_string()))
        .unwrap();
    send(app, req).await
}

async fn v1_get(app: &Router, caller: &str, path: &str) -> Reply {
    let req = Request::get(path)
        .header("x-forwarded-for", caller)
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn page_run(app: &Router, caller: &str, tool: &str, form: &str) -> Reply {
    let req = Request::post(format!("/inspect/{tool}/run"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("x-forwarded-for", caller)
        .body(Body::from(form.to_string()))
        .unwrap();
    send(app, req).await
}

fn assert_page_refused(r: &Reply, what: &str) {
    assert_eq!(
        r.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{what}: {}",
        r.body
    );
    let retry: u64 = r.headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(retry >= 1, "{what}");
    assert!(r.body.contains("too many requests"), "{what}: {}", r.body);
}

/// Posts to v1 until it answers 429; at most `max` tries, each earlier answer is 200.
async fn exhaust_post(app: &Router, caller: &str, path: &str, json: &str, max: usize) {
    for _ in 0..max {
        let r = v1_post(app, caller, path, json).await;
        if r.status == StatusCode::TOO_MANY_REQUESTS {
            return;
        }
        assert_eq!(r.status, StatusCode::OK, "{path}: {}", r.body);
    }
    panic!("{path}: v1 never answered 429 within {max} requests");
}

/// C15: the per-target budget for example.com is spent through v1; a different caller on the
/// page is refused and no handshake happens for it.
#[tokio::test]
async fn tls_per_target_budget_spent_on_v1_refuses_the_page() {
    let config: ApiConfig = toml::from_str(
        "[tls]\nper_target_burst = 2\nper_target_per_minute = 1\nper_ip_burst = 1000\nper_ip_per_minute = 1000\n",
    )
    .expect("[api.tls] keys parse");
    let backend = Arc::new(
        StubTlsBackend::new().with_host("example.com", vec!["192.0.2.50".parse().unwrap()]),
    );
    let parts = ApiParts {
        tls: Some(stub_state(backend.clone()).inspector()),
        ..Default::default()
    };
    let (v1, pages) = setup(config, parts);

    exhaust_post(
        &v1,
        "198.51.100.1",
        "/tls/inspect",
        r#"{"host":"example.com"}"#,
        5,
    )
    .await;
    let before = backend.calls().len();

    let r = page_run(&pages, "198.51.100.2", "tls", "h=example.com").await;
    assert_page_refused(&r, "tls");
    assert_eq!(
        backend.calls().len(),
        before,
        "the refused page request handshook"
    );
}

/// C6: the same for the `[api.http]` per-target budget.
#[tokio::test]
async fn http_per_target_budget_spent_on_v1_refuses_the_page() {
    let config: ApiConfig = toml::from_str(
        "[http]\nper_target_burst = 2\nper_target_per_minute = 1\nper_ip_burst = 1000\nper_ip_per_minute = 1000\n",
    )
    .expect("[api.http] keys parse");
    let parts = ApiParts {
        http: Some(Arc::new(StubHttpInspect::new())),
        ..Default::default()
    };
    let (v1, pages) = setup(config, parts);

    exhaust_post(
        &v1,
        "198.51.100.1",
        "/http/inspect",
        r#"{"url":"https://example.com/"}"#,
        5,
    )
    .await;

    let r = page_run(
        &pages,
        "198.51.100.2",
        "http",
        "url=https%3A%2F%2Fexample.com%2F",
    )
    .await;
    assert_page_refused(&r, "http");
}

/// C6: the `[api.ip]` per-client budget is spent through v1; the page refuses that client and
/// still serves another.
#[tokio::test]
async fn ip_per_client_budget_spent_on_v1_refuses_that_client_on_the_page() {
    let config: ApiConfig =
        toml::from_str("[ip]\ninternal_mode = true\nper_ip_burst = 2\nper_ip_per_minute = 1\n")
            .expect("[api.ip] keys parse");
    let parts = ApiParts {
        ip: Some(Arc::new(StubIpLookup::new())),
        ..Default::default()
    };
    let (v1, pages) = setup(config, parts);

    let mut refused = false;
    for _ in 0..5 {
        let r = v1_get(&v1, "192.0.2.7", "/ip/192.0.2.1").await;
        if r.status == StatusCode::TOO_MANY_REQUESTS {
            refused = true;
            break;
        }
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    }
    assert!(refused, "v1 never answered 429 for the spent client");

    let same = page_run(&pages, "192.0.2.7", "ip", "ip=192.0.2.2").await;
    assert_page_refused(&same, "ip, same client");

    let other = page_run(&pages, "192.0.2.8", "ip", "ip=192.0.2.2").await;
    assert_eq!(other.status, StatusCode::OK, "{}", other.body);
}
