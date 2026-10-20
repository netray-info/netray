//! V2 spec v2-site-nav, Phase 3 (requirements 10, 11): the footer's markup and styles.
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
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

async fn pages_get(path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    get_on(pages::router().fallback(pages::not_found), path).await
}

async fn inspect(path: &str) -> String {
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let (status, _, body) = get_on(app, path).await;
    assert_eq!(status, StatusCode::OK, "{path}");
    body
}

/// Every frame page the footer appears on, as (label, html, compare link expected in main).
async fn frame_pages() -> Vec<(&'static str, String, bool)> {
    vec![
        ("/guide/csp", pages_get("/guide/csp").await.2, false),
        ("/tools", pages_get("/tools").await.2, false),
        ("/inspect/dns", inspect("/inspect/dns").await, false),
        ("privacy", render::privacy().unwrap(), false),
        ("404", pages_get("/nope").await.2, true),
    ]
}

fn in_order(html: &str, needles: &[String], ctx: &str) {
    let mut from = 0;
    for n in needles {
        match html[from..].find(n.as_str()) {
            Some(at) => from += at + n.len(),
            None => panic!("{ctx}: `{n}` missing or out of order"),
        }
    }
}

fn footer_needles() -> Vec<String> {
    let version = env!("CARGO_PKG_VERSION");
    vec![
        r#"<footer class="page-footer">"#.to_string(),
        r#"<div class="page-footer__inner">"#.to_string(),
        r#"<div class="page-footer__about">"#.to_string(),
        "<p>Every check of a domain, in one spectrum.</p>".to_string(),
        format!(r#"<span class="page-footer__version">netray {version}</span>"#),
        r#"<nav class="page-footer__nav" aria-label="Footer">"#.to_string(),
        r#"<div class="page-footer__group"><span class="page-footer__heading">Inspectors</span><div class="page-footer__links">"#.to_string(),
        r#"<a href="/inspect/dns">DNS</a><a href="/inspect/tls">TLS</a><a href="/inspect/http">HTTP</a><a href="/inspect/email">Email</a><a href="/inspect/ip">IP</a></div></div>"#.to_string(),
        r#"<div class="page-footer__group"><span class="page-footer__heading">Site</span><div class="page-footer__links">"#.to_string(),
        r#"<a href="/guide/">Guides</a><a href="/compare">Compare</a><a href="/status">Status</a><a href="/api/v1/docs">API docs</a></div></div>"#.to_string(),
        r#"<div class="page-footer__group"><span class="page-footer__heading">About</span><div class="page-footer__links">"#.to_string(),
        r#"<a href="/privacy">Privacy</a><a href="https://github.com/netray-info/netray" rel="noopener">GitHub</a></div></div>"#.to_string(),
        "</nav>".to_string(),
        "</footer>".to_string(),
    ]
}

#[tokio::test]
async fn c1_c3_c4_the_footer_holds_tagline_version_and_three_link_groups() {
    let needles = footer_needles();
    for (ctx, html, _) in frame_pages().await {
        in_order(&html, &needles, ctx);
    }
}

#[tokio::test]
async fn c1_c5_compare_is_in_the_footer_once_and_not_in_the_header() {
    for (ctx, html, in_main) in frame_pages().await {
        let header = html
            .split("<header")
            .nth(1)
            .and_then(|h| h.split("</header>").next())
            .unwrap_or("");
        assert!(
            !header.contains("/compare"),
            "{ctx}: /compare in the header"
        );
        let footer = html
            .split(r#"<footer class="page-footer">"#)
            .nth(1)
            .unwrap_or("");
        assert!(
            footer.contains(r#"<a href="/compare">Compare</a><a href="/status">"#),
            "{ctx}: Compare not in the footer's Site group"
        );
        let want = if in_main { 2 } else { 1 };
        assert_eq!(
            html.matches(r#"href="/compare""#).count(),
            want,
            "{ctx}: href=\"/compare\" count"
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
        if squash(&rest[..open])
            .split(',')
            .any(|s| s == squash(selector))
        {
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

fn pages_css() -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static/pages.css")).unwrap()
}

#[test]
fn c1_c6_the_footer_desktop_styles_resolve_to_the_board_values() {
    let css = pages_css();
    let rows = [
        (".page-footer__inner", "display:flex"),
        (".page-footer__inner", "flex-wrap:wrap"),
        (".page-footer__inner", "justify-content:space-between"),
        (".page-footer__inner", "gap:24px48px"),
        (".page-footer__inner", "padding-top:24px"),
        (".page-footer__nav", "gap:20px48px"),
        (".page-footer__group", "flex-direction:column"),
        (".page-footer__group", "gap:6px"),
        (".page-footer__links", "gap:4px18px"),
        (".page-footer__heading", "font-family:var(--font-mono)"),
        (
            ".page-footer__heading",
            "letter-spacing:var(--font-tracking-caps)",
        ),
        (".page-footer__heading", "text-transform:uppercase"),
        (".page-footer__version", "font-family:var(--font-mono)"),
    ];
    for (selector, decl) in rows {
        assert!(
            declared(&blocks_in(&css, selector), decl),
            "{selector} lacks {decl}"
        );
    }
}

#[test]
fn c2_c6_the_footer_is_one_column_on_a_phone_with_44px_links() {
    let regions = phone_regions(&pages_css());
    let rows = [
        (".page-footer__inner", "flex-direction:column"),
        (".page-footer__inner", "gap:20px"),
        (".page-footer__inner", "padding-top:20px"),
        (".page-footer__about", "display:contents"),
        (".page-footer__version", "order:2"),
        (".page-footer a", "min-height:44px"),
    ];
    for (selector, decl) in rows {
        let blocks: Vec<String> = regions
            .iter()
            .flat_map(|r| blocks_in(r, selector))
            .collect();
        assert!(declared(&blocks, decl), "phone: {selector} lacks {decl}");
    }
}

/// Reader, phase 3: the expired-entry page inlines its sheet; the footer's rules are in it.
#[test]
fn c1_the_gone_page_inlines_the_footer_styles() {
    let html = render::gone().unwrap();
    let style = &html[html.find("<style>").unwrap()..html.find("</style>").unwrap()];
    for selector in [
        ".page-footer__inner",
        ".page-footer__links",
        ".page-footer__heading",
    ] {
        assert!(style.contains(selector), "gone page sheet lacks {selector}");
    }
    assert!(html.contains(r#"<nav class="page-footer__nav" aria-label="Footer">"#));
}
