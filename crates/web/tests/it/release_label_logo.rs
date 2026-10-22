//! Badges and OG cards carry "netray", and the header logo leads to the apex on every host
//! (specs/features/v2-release-cleanup, Phase 2, requirements 6 and 7).

use std::net::SocketAddr;
use std::time::{Duration, SystemTime};

use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use netray_engine::Registry;
use netray_web::api::ApiParts;
use netray_web::config::{ApexConfig, BadgesConfig};
use netray_web::host::{HostRouters, build, probes, router};
use tower::ServiceExt;

const BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64) Gecko/20100101 Firefox/130.0";
const LOGO: &str = r#"<a href="https://netray.info/"><img src="/static/logo.svg""#;

fn hosts() -> Router {
    let apex = build(
        ApexConfig::default(),
        Registry::new(),
        Vec::new(),
        Duration::from_millis(1500),
        netray_engine::profile::Profiles::current().clone(),
        None,
        ApiParts::default(),
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
        [127, 0, 0, 1],
        40000,
    )))))
}

async fn get(app: &Router, host: &str, path: &str) -> (StatusCode, String) {
    let req = Request::builder()
        .uri(path)
        .header(header::HOST, host)
        .header(header::USER_AGENT, BROWSER)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[test]
fn r6_the_default_badge_label_is_netray() {
    assert_eq!(BadgesConfig::default().default_label, "netray");
}

#[test]
fn r6_the_og_card_label_is_netray() {
    assert_eq!(netray_web::og::layout::DEFAULT_LABEL, "netray");
    let svg = netray_web::og::render::svg_for_grade(
        "example.com",
        "A",
        netray_web::og::layout::DEFAULT_LABEL,
        91.0,
        SystemTime::UNIX_EPOCH,
        None,
    );
    assert!(svg.contains("netray \u{00b7} netray.info"), "footer: {svg}");
    assert!(!svg.contains("lens"), "no lens label: {svg}");
}

#[tokio::test]
async fn r6_a_default_badge_says_netray_and_a_custom_label_stays() {
    let app = hosts();
    let (status, body) = get(&app, "netray.info", "/badge/example.com.svg").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(">netray<"), "default label: {body}");
    assert!(!body.contains("lens"), "no lens label: {body}");
    let (_, body) = get(&app, "netray.info", "/badge/example.com.svg?label=custom").await;
    assert!(body.contains(">custom<"), "custom label: {body}");
}

#[tokio::test]
async fn r7_the_logo_links_to_the_apex_on_apex_pages() {
    let app = hosts();
    for path in ["/", "/inspect/dns", "/tools", "/guide/"] {
        let (status, body) = get(&app, "netray.info", path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(body.contains(LOGO), "{path}: logo must link to the apex");
        assert!(
            !body.contains(r#"<a href="/"><img"#),
            "{path}: a host-relative logo link remains"
        );
    }
}

#[tokio::test]
async fn r7_the_inspector_hosts_logo_links_to_the_apex_with_the_apex_bytes() {
    let app = hosts();
    for tool in ["dns", "tls", "http", "ip"] {
        let (status, body) = get(&app, &format!("{tool}.netray.info"), "/").await;
        assert_eq!(status, StatusCode::OK, "{tool}.");
        assert!(body.contains(LOGO), "{tool}.: logo must link to the apex");
        let (_, apex) = get(&app, "netray.info", &format!("/inspect/{tool}")).await;
        assert_eq!(
            body, apex,
            "{tool}.: must serve the apex's /inspect/{tool} bytes"
        );
    }
}
