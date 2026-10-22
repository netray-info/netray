//! v2-cutover, operator decision 2026-10-22: on `ip.`, `netray_web::api::ip_host` answers before
//! the `410` rows of requirement 14. The field and format family it serves (S40) answers again;
//! what it hands back still meets the rows, and a browser on `/` still gets the inspector.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode, header};
use netray_engine::Registry;
use netray_ip::testing::StubIpLookup;
use netray_web::api::ApiParts;
use netray_web::config::ApexConfig;
use netray_web::host::{HostRouters, build, probes, router};
use tower::ServiceExt;

const CURL: &str = "curl/8.5.0";
const BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64) Gecko/20100101 Firefox/130.0";
const BROWSER_ACCEPT: &str = "text/html,application/xhtml+xml,*/*;q=0.8";

fn app() -> Router {
    let apex = build(
        ApexConfig::default(),
        Registry::new(),
        Vec::new(),
        Duration::from_millis(1500),
        netray_engine::profile::Profiles::current().clone(),
        None,
        ApiParts {
            ip: Some(Arc::new(StubIpLookup::new())),
            ..Default::default()
        },
    );
    router(HostRouters {
        apex: "netray.info".into(),
        probes: probes(),
        apex_router: apex.apex_router,
        dns: Some(apex.dns),
        tls: Some(apex.tls),
        http: Some(apex.http),
        ip: Some(apex.ip),
        ip_api: Some(apex.api),
    })
    .layer(Extension(ConnectInfo(SocketAddr::from((
        [192, 0, 2, 10],
        40000,
    )))))
}

async fn send(method: Method, path: &str, ua: &str, accept: Option<&str>) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "ip.netray.info")
        .header(header::USER_AGENT, ua);
    if let Some(accept) = accept {
        req = req.header(header::ACCEPT, accept);
    }
    let resp = app()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn ip_host_answers_its_paths_before_the_gone_rows() {
    for path in [
        "/json",
        "/country",
        "/country/json",
        "/ip/cidr",
        "/all/yaml",
        "/asn",
    ] {
        let (status, body) = send(Method::GET, path, CURL, None).await;
        assert_ne!(
            status,
            StatusCode::GONE,
            "{path} reached the 410 rows: {body}"
        );
        assert_ne!(
            status,
            StatusCode::MOVED_PERMANENTLY,
            "{path} moved to the apex"
        );
    }
}

#[tokio::test]
async fn curl_on_root_gets_the_address_as_text() {
    let (status, body) = send(Method::GET, "/", CURL, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("<html"), "{body}");
}

#[tokio::test]
async fn paths_ip_host_hands_back_still_meet_the_rows() {
    for (method, path, successor) in [
        (Method::GET, "/range", "/api/v1/ip/range/{cidr}"),
        (Method::GET, "/diff", "/api/v1/ip/diff"),
        (Method::POST, "/batch", "/api/v1/ip/batch"),
        (Method::GET, "/meta", "/api/v1/meta"),
        (Method::GET, "/ipv4", "/api/v1/ip/{address}"),
        (Method::GET, "/asn/13335", "/api/v1/asn/{n}"),
        (Method::POST, "/json", "/api/v1/ip"),
    ] {
        let (status, body) = send(method.clone(), path, CURL, None).await;
        assert_eq!(status, StatusCode::GONE, "{method} {path}: {body}");
        assert!(body.contains(successor), "{method} {path}: {body}");
    }
}

#[tokio::test]
async fn a_browser_on_root_gets_the_inspector() {
    let (status, body) = send(Method::GET, "/", BROWSER, Some(BROWSER_ACCEPT)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"data-stream-url="/inspect/ip/run""#),
        "not the inspector"
    );
}
