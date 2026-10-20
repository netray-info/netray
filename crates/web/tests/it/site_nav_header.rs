//! V2 spec v2-site-nav, Phase 2 (requirements 5-9): the header's Inspectors menu, the /tools
//! page, the privacy page in the shared frame, the inspector taglines and the menu's styles.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use netray_web::entries::{privacy_router, render};
use netray_web::pages::{
    self, InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, guides, inspect_router,
};
use tower::ServiceExt;

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        Ok(Job::new(vec![]))
    }
}

async fn get_on(app: Router, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let res = app
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8_lossy(&bytes).into_owned())
}

async fn pages_get(path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    get_on(pages::router(), path).await
}

async fn inspect(path: &str) -> String {
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let (status, _, body) = get_on(app, path).await;
    assert_eq!(status, StatusCode::OK, "{path}");
    body
}

/// Every frame page the header nav appears on, as (label, html).
async fn frame_pages() -> Vec<(&'static str, String)> {
    vec![
        ("/guide/csp", pages_get("/guide/csp").await.2),
        ("/tools", pages_get("/tools").await.2),
        ("/inspect/dns", inspect("/inspect/dns").await),
        ("privacy", render::privacy().unwrap()),
    ]
}

const TOOLS: [(&str, &str, &str); 5] = [
    ("dns", "DNS", "DNS records and resolvers"),
    ("tls", "TLS", "TLS certificates and handshake"),
    ("http", "HTTP", "HTTP headers and redirects"),
    ("email", "Email", "Email MX, SPF, DKIM, DMARC"),
    ("ip", "IP", "IP location and network"),
];

/// Positions of each needle, in order; panics naming the first one missing or out of order.
fn in_order(html: &str, needles: &[String], ctx: &str) {
    let mut from = 0;
    for n in needles {
        match html[from..].find(n.as_str()) {
            Some(at) => from += at + n.len(),
            None => panic!("{ctx}: `{n}` missing or out of order"),
        }
    }
}

#[tokio::test]
async fn c6_the_header_nav_is_the_menu_then_guides_api_status() {
    let mut needles = vec![
        r#"<nav class="page-nav" data-island="menu">"#.to_string(),
        r#"<details class="nav-menu"#.to_string(),
        r#"<summary class="nav-menu__button">Inspectors"#.to_string(),
        "<svg".to_string(),
        r#"<div class="nav-menu__panel" id="inspectors-menu">"#.to_string(),
    ];
    for (tool, label, desc) in TOOLS {
        needles.push(format!(r#"<a class="nav-menu__item" href="/inspect/{tool}""#));
        needles.push(format!(
            r#"<span class="nav-menu__label">{label}</span><span class="nav-menu__desc">{desc}</span>"#
        ));
    }
    needles.extend(
        [
            r#"<a class="nav-menu__all" href="/tools">All tools →</a>"#,
            "</details>",
            r#"<a href="/guide/">Guides</a>"#,
            r#"<a href="/api/v1/docs">API</a>"#,
            r#"<a href="/status">Status</a>"#,
        ]
        .map(String::from),
    );
    for (ctx, html) in frame_pages().await {
        in_order(&html, &needles, ctx);
        let nav = html.split(r#"<nav class="page-nav""#).nth(1).unwrap_or("");
        let nav = nav.split("</nav>").next().unwrap();
        assert!(!nav.contains("Compare"), "{ctx}: Compare in the header nav");
        assert!(
            !html.contains(r#"href="/inspect/dns">Inspectors</a>"#),
            "{ctx}: the old plain Inspectors link"
        );
    }
}

#[tokio::test]
async fn c7_the_menu_marks_the_current_inspector_and_the_tools_page() {
    let tls = inspect("/inspect/tls").await;
    assert!(tls.contains(r#"<details class="nav-menu nav-menu--current""#));
    assert!(tls.contains(r#"<a class="nav-menu__item" href="/inspect/tls" aria-current="page">"#));
    assert_eq!(tls.matches("nav-menu__item").count(), 5);
    assert_eq!(
        tls.matches(r#"class="nav-menu__item" href="/inspect/"#).count(),
        5
    );
    assert_eq!(tls.matches(r#" aria-current="page">"#).count() >= 1, true);
    for other in ["dns", "http", "email", "ip"] {
        assert!(
            !tls.contains(&format!(r#"href="/inspect/{other}" aria-current="page""#)),
            "{other} marked on the TLS page"
        );
    }

    let tools = pages_get("/tools").await.2;
    assert!(tools.contains(r#"<details class="nav-menu nav-menu--current""#));
    assert!(!tools.contains(r#"class="nav-menu__item" href="/inspect/dns" aria-current"#));
    for (tool, _, _) in TOOLS {
        assert!(!tools.contains(&format!(r#"href="/inspect/{tool}" aria-current="page""#)));
    }

    let guide = pages_get("/guide/csp").await.2;
    assert!(guide.contains(r#"<details class="nav-menu">"#));
    assert!(!guide.contains("nav-menu--current"));
}

#[tokio::test]
async fn c8_tools_page_lists_five_inspector_cards_and_featured_guides() {
    let (status, _, html) = pages_get("/tools").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("<h1>Inspectors</h1>"));
    let title = html.split("<title>").nth(1).unwrap().split("</title>").next().unwrap();
    assert!(title.contains("Inspectors"), "title: {title}");

    let cards: Vec<&str> = html.split(r#"<article class="tool-card">"#).skip(1).collect();
    assert_eq!(cards.len(), 5);
    let rows = [
        ("dns", "DNS", "DNS, resolved", "a name and record types", "q"),
        ("tls", "TLS", "TLS, illuminated", "a host and ports", "h"),
        ("http", "HTTP", "HTTP, inspected", "a URL", "url"),
        ("email", "Email", "Email, verified", "a domain and DKIM selectors", "d"),
        ("ip", "IP", "IP, located", "an address, or none for yours", "ip"),
    ];
    for (card, (tool, name, tagline, takes, field)) in cards.iter().zip(rows) {
        let card = card.split("</article>").next().unwrap();
        assert!(card.contains(&format!("<h2>{name} inspector</h2>")), "{tool}");
        assert!(
            card.contains(&format!(r#"<p class="tool-card__tagline">{tagline}</p>"#)),
            "{tool}"
        );
        assert!(
            card.contains(&format!(r#"<p class="tool-card__takes">{takes}</p>"#)),
            "{tool}"
        );
        assert!(card.contains(&format!(r#"action="/inspect/{tool}""#)), "{tool}");
        assert!(card.contains(&format!(r#"name="{field}""#)), "{tool}");
        assert!(
            card.contains(&format!(r#"<a href="/inspect/{tool}">Open the {name} inspector →</a>"#)),
            "{tool}"
        );
    }

    let after = html.split("<h2>Featured guides</h2>").nth(1).expect("Featured guides");
    assert!(after.matches(r#"href="/guide/"#).count() >= 10);
    for g in guides::GUIDES {
        assert!(after.contains(&format!(r#"href="/guide/{}""#, g.slug)), "{}", g.slug);
    }
    let last_guide = after.rfind(r#"href="/guide/"#).unwrap();
    let api = after.find(r#"href="/api/v1/docs""#).expect("API link");
    assert!(api > last_guide);
}

#[tokio::test]
async fn c9_tools_with_a_trailing_slash_redirects_permanently() {
    let (status, headers, _) = pages_get("/tools/").await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(headers.get(header::LOCATION).unwrap(), "/tools");
}

#[tokio::test]
async fn c11_privacy_renders_in_the_shared_frame() {
    let (status, _, routed) = get_on(privacy_router(), "/privacy").await;
    assert_eq!(status, StatusCode::OK);
    for html in [render::privacy().unwrap(), routed] {
        assert!(html.contains(r#"<header class="page-header""#));
        assert!(html.contains(r#"<footer class="page-footer""#));
        assert!(html.contains(r#"<link rel="stylesheet" href="/static/app.css">"#));
        assert!(html.contains("<title>Privacy · netray</title>"));
        assert!(!html.contains("<style"));
    }
}

#[tokio::test]
async fn c12_each_inspector_page_carries_its_tagline() {
    for (tool, _, _) in TOOLS {
        let tagline = match tool {
            "dns" => "DNS, resolved",
            "tls" => "TLS, illuminated",
            "http" => "HTTP, inspected",
            "email" => "Email, verified",
            _ => "IP, located",
        };
        let html = inspect(&format!("/inspect/{tool}")).await;
        assert!(
            html.contains(&format!(r#"<span class="page-tagline">{tagline}</span>"#)),
            "{tool}"
        );
    }
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn all_css() -> Vec<String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "css") {
            out.push(fs::read_to_string(p).unwrap());
        }
    }
    out
}

/// Declaration blocks (squashed) of rules whose selector list holds `selector` exactly.
fn blocks_in(css: &str, selector: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        if squash(&rest[..open]).split(',').any(|s| s == squash(selector)) {
            out.push(squash(&rest[open + 1..open + close]));
        }
        rest = &rest[open + close + 1..];
    }
    out
}

fn phone_regions(css: &str) -> Vec<String> {
    let marker = "@media (max-width: 480px)";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = css[from..].find(marker) {
        let start = from + at;
        let Some(open) = css[start..].find('{') else {
            break;
        };
        let body = start + open + 1;
        let (mut depth, mut end) = (1, css.len());
        for (i, c) in css[body..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = body + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(css[body..end].to_string());
        from = end;
    }
    out
}

fn declared(blocks: &[String], decl: &str) -> bool {
    blocks.iter().any(|b| b.split(';').any(|d| d == decl))
}

#[test]
fn c13_the_menu_styles_resolve_to_the_board_values() {
    let css = all_css();
    let rows = [
        (".nav-menu__panel", "width:400px"),
        (".nav-menu__panel", "padding:6px"),
        (".nav-menu__item", "min-height:44px"),
        (".nav-menu__item", "padding:8px12px"),
        (".nav-menu__label", "font-family:var(--font-mono)"),
        (".nav-menu__label", "text-transform:uppercase"),
    ];
    for (selector, decl) in rows {
        let blocks: Vec<String> = css.iter().flat_map(|c| blocks_in(c, selector)).collect();
        assert!(declared(&blocks, decl), "{selector} lacks {decl}");
    }
}

#[test]
fn c14_the_menu_panel_is_static_inside_the_phone_query() {
    let blocks: Vec<String> = all_css()
        .iter()
        .flat_map(|c| phone_regions(c))
        .flat_map(|r| blocks_in(&r, ".nav-menu__panel"))
        .collect();
    assert!(declared(&blocks, "position:static"), "phone blocks: {blocks:?}");
}
