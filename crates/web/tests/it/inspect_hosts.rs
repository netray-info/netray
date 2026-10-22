//! v2-cutover requirement 2 (C8, C9, C13) at the router: `host::build` gives `dns.`, `tls.`,
//! `http.` and `ip.` their inspector at `/` through the handler of the apex's `/inspect/<tool>`;
//! a V1 query string prefills it, `r` is ignored, the run endpoint answers on the host, and
//! `ip.` answers a command-line client with the caller's address as text, behind the trusted
//! proxy. `crates/netray/tests/serve_inspector_hosts.rs` holds the same scenarios over the binary.

use std::net::SocketAddr;
use std::time::Duration;

use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use netray_engine::Registry;
use netray_web::api::ApiParts;
use netray_web::config::ApexConfig;
use netray_web::host::{Apex, HostRouters, build, probes, router};
use tower::ServiceExt;

const BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64) Gecko/20100101 Firefox/130.0";
const TOOLS: [(&str, &str); 4] = [("dns", "q"), ("tls", "h"), ("http", "url"), ("ip", "ip")];

fn apex(trusted: &[&str]) -> Apex {
    build(
        ApexConfig {
            trusted_proxies: trusted.iter().map(|s| s.to_string()).collect(),
            ..ApexConfig::default()
        },
        Registry::new(),
        Vec::new(),
        Duration::from_millis(1500),
        netray_engine::profile::Profiles::current().clone(),
        None,
        ApiParts::default(),
    )
}

fn hosts(apex: Apex, peer: [u8; 4]) -> Router {
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
    // The inspector handlers read the `ConnectInfo` extension `into_make_service_with_connect_info`
    // sets, so the test sets it the same way.
    .layer(Extension(ConnectInfo(SocketAddr::from((peer, 40000)))))
}

struct Answer {
    status: StatusCode,
    content_type: String,
    body: String,
}

async fn get(app: &Router, host: &str, path: &str, ua: &str, extra: &[(&str, &str)]) -> Answer {
    let mut req = Request::builder()
        .uri(path)
        .header(header::HOST, host)
        .header(header::USER_AGENT, ua);
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    Answer {
        status,
        content_type,
        body: String::from_utf8_lossy(&body).into_owned(),
    }
}

fn marker(tool: &str) -> String {
    format!(r#"data-stream-url="/inspect/{tool}/run""#)
}

#[tokio::test]
async fn c8_each_inspector_host_serves_its_inspector_at_root() {
    let app = hosts(apex(&[]), [127, 0, 0, 1]);
    for (tool, field) in TOOLS {
        let host = format!("{tool}.netray.info");
        let r = get(&app, &host, "/", BROWSER, &[]).await;
        assert_eq!(r.status, StatusCode::OK, "{host} /");
        assert!(r.content_type.starts_with("text/html"), "{host}");
        assert!(
            r.body.contains(&marker(tool)),
            "{host}: not the {tool} inspector"
        );
        assert!(
            r.body.contains(&format!(r#"name="{field}""#)),
            "{host}: no input {field}"
        );
        assert!(!r.body.contains("data-stream-autostart"), "{host}");
    }
    let via_apex = get(
        &app,
        "netray.info",
        "/inspect/dns?q=example.com",
        BROWSER,
        &[],
    )
    .await;
    let via_host = get(&app, "dns.netray.info", "/?q=example.com", BROWSER, &[]).await;
    assert_eq!(via_apex.status, StatusCode::OK);
    assert_eq!(via_host.status, StatusCode::OK);
    assert_eq!(via_host.body, via_apex.body, "same handler, same body");
}

#[tokio::test]
async fn c9_v1_query_strings_prefill_and_r_is_ignored() {
    let app = hosts(apex(&[]), [127, 0, 0, 1]);
    let rows = [
        ("dns", "/?q=example.com", "example.com"),
        ("tls", "/?h=example.com", "example.com"),
        ("http", "/?url=https://example.com/", "https://example.com/"),
        ("ip", "/?ip=192.0.2.1", "192.0.2.1"),
    ];
    for (tool, path, value) in rows {
        let host = format!("{tool}.netray.info");
        let r = get(&app, &host, path, BROWSER, &[]).await;
        assert_eq!(r.status, StatusCode::OK, "{host}{path}");
        assert!(r.body.contains(&marker(tool)), "{host}{path}");
        assert!(
            r.body.contains(&format!(r#"value="{value}""#)),
            "{host}{path}: not prefilled"
        );
    }
    let plain = get(&app, "dns.netray.info", "/?q=example.com", BROWSER, &[]).await;
    let with_r = get(
        &app,
        "dns.netray.info",
        "/?q=example.com&r=k1",
        BROWSER,
        &[],
    )
    .await;
    assert_eq!(with_r.status, StatusCode::OK);
    assert_eq!(with_r.body, plain.body, "r is ignored");
}

#[tokio::test]
async fn c9_ip_root_answers_cli_clients_with_the_caller() {
    let app = hosts(apex(&[]), [127, 0, 0, 1]);
    for ua in ["curl/8.0", "Wget/1.21", "HTTPie/3.2.1"] {
        let r = get(&app, "ip.netray.info", "/", ua, &[]).await;
        assert_eq!(r.status, StatusCode::OK, "{ua}");
        assert!(
            r.content_type.starts_with("text/plain"),
            "{ua}: {}",
            r.content_type
        );
        assert_eq!(r.body.trim(), "127.0.0.1", "{ua}");
    }
    let accepts_all = get(
        &app,
        "ip.netray.info",
        "/",
        "curl/8.0",
        &[("accept", "*/*")],
    )
    .await;
    assert_eq!(accepts_all.body.trim(), "127.0.0.1");
    let wants_html = get(
        &app,
        "ip.netray.info",
        "/",
        "curl/8.0",
        &[("accept", "text/html")],
    )
    .await;
    assert!(
        wants_html.body.contains(&marker("ip")),
        "Accept: text/html gets the page"
    );
    let browser = get(&app, "ip.netray.info", "/", BROWSER, &[]).await;
    assert_eq!(browser.status, StatusCode::OK);
    assert!(browser.content_type.starts_with("text/html"));
    assert!(browser.body.contains(&marker("ip")));
}

#[tokio::test]
async fn c9_ip_root_names_the_client_behind_a_trusted_proxy() {
    let app = hosts(apex(&["10.0.0.0/8"]), [10, 0, 0, 2]);
    let r = get(
        &app,
        "ip.netray.info",
        "/",
        "curl/8.0",
        &[("x-forwarded-for", "198.51.100.7")],
    )
    .await;
    assert_eq!(r.body.trim(), "198.51.100.7");
    let spoofed = hosts(apex(&[]), [192, 0, 2, 9]);
    let r = get(
        &spoofed,
        "ip.netray.info",
        "/",
        "curl/8.0",
        &[("x-forwarded-for", "198.51.100.7")],
    )
    .await;
    assert_eq!(
        r.body.trim(),
        "192.0.2.9",
        "an untrusted peer's header is ignored"
    );
}

#[tokio::test]
async fn c13_inspector_hosts_serve_their_run_endpoint_assets_and_404s() {
    let app = hosts(apex(&[]), [127, 0, 0, 1]);
    let run = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/inspect/dns/run")
                .header(header::HOST, "dns.netray.info")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("q="))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        run.status(),
        StatusCode::NOT_FOUND,
        "the host's run endpoint"
    );
    let other_tool = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/inspect/tls/run")
                .header(header::HOST, "dns.netray.info")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("h=example.com"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        other_tool.status(),
        StatusCode::NOT_FOUND,
        "another tool's run"
    );

    let page = get(&app, "dns.netray.info", "/", BROWSER, &[]).await;
    let asset = page
        .body
        .split('"')
        .find(|s| s.starts_with("/static/") && s.ends_with(".css"))
        .expect("the page links a stylesheet under /static/")
        .to_owned();
    let css = get(&app, "dns.netray.info", &asset, BROWSER, &[]).await;
    assert_eq!(css.status, StatusCode::OK, "{asset} on the host");

    // The host's pages are the apex's, links included: an apex-only path moves to the apex.
    for path in [
        "/guide/",
        "/example.com",
        "/inspect/dns?q=example.com",
        "/nope",
    ] {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(header::HOST, "dns.netray.info")
                    .header(header::USER_AGENT, BROWSER)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::MOVED_PERMANENTLY,
            "dns.netray.info{path}"
        );
        assert_eq!(
            resp.headers()[header::LOCATION],
            format!("https://netray.info{path}").as_str(),
            "dns.netray.info{path}"
        );
    }
    assert_eq!(
        get(&app, "dns.netray.info", "/health", BROWSER, &[])
            .await
            .status,
        StatusCode::OK
    );
}
