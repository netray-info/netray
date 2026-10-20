//! V2 spec v2-site-nav, Phase 1: every absolute URL is built from the apex origin
//! `https://netray.info`, and links and form actions between pages are apex-relative.
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages::{self, guides};
use regex::Regex;
use tower::ServiceExt;

fn app() -> Router {
    Router::new()
        .route("/robots.txt", get(pages::robots))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn page(path: &str) -> String {
    let res = app()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "{path}");
    String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

fn subdomain_urls(text: &str) -> Vec<String> {
    Regex::new(r"https://[a-z0-9-]+\.netray\.info")
        .unwrap()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn locs(xml: &str) -> Vec<String> {
    Regex::new(r"<loc>([^<]*)</loc>")
        .unwrap()
        .captures_iter(xml)
        .map(|c| c[1].to_string())
        .collect()
}

#[tokio::test]
async fn c1_every_absolute_url_is_built_from_the_apex_origin() {
    for path in ["/guide/csp", "/guide/", "/sitemap.xml", "/robots.txt"] {
        let text = page(path).await;
        assert_eq!(subdomain_urls(&text), Vec::<String>::new(), "{path}");
        for (at, _) in text.match_indices("https://netray.info") {
            let rest = &text[at + "https://netray.info".len()..];
            assert!(rest.starts_with('/'), "{path}: apex URL without a path at {at}");
        }
    }
}

#[tokio::test]
async fn c2_guide_links_and_forms_are_apex_relative() {
    let action = Regex::new(r#"<form[^>]*\saction="([^"]*)""#).unwrap();
    for g in guides::GUIDES {
        let html = page(&format!("/guide/{}", g.slug)).await;
        assert!(
            !html.contains("href=\"https://netray.info"),
            "{}: absolute apex href",
            g.slug
        );
        assert!(!html.contains("action=\"https://"), "{}: absolute action", g.slug);
        for c in action.captures_iter(&html) {
            if let Some(tool) = c[1].strip_prefix("/inspect/") {
                assert!(
                    ["dns", "tls", "http", "email", "ip"].contains(&tool),
                    "{}: unknown tool in {}",
                    g.slug,
                    &c[1]
                );
            }
        }
    }
}

#[tokio::test]
async fn c4_sitemap_lists_the_apex_pages_and_inspectors() {
    let l = locs(&page("/sitemap.xml").await);
    for p in [
        "/", "/guide/", "/compare", "/status", "/privacy", "/tools", "/api/v1/docs",
        "/inspect/dns", "/inspect/tls", "/inspect/http", "/inspect/email", "/inspect/ip",
    ] {
        let want = format!("https://netray.info{p}");
        assert!(l.contains(&want), "missing loc {want}");
    }
    let subs: Vec<_> = l.iter().filter(|u| !subdomain_urls(u).is_empty()).collect();
    assert!(subs.is_empty(), "subdomain locs: {subs:?}");
}

#[tokio::test]
async fn c5_csp_guide_is_canonical_and_its_form_goes_to_the_apex_inspector() {
    let html = page("/guide/csp").await;
    assert!(html.contains(r#"<link rel="canonical" href="https://netray.info/guide/csp">"#));
    assert!(html.contains(r#"action="/inspect/http""#), "no /inspect/http form");
    assert_eq!(subdomain_urls(&html), Vec::<String>::new());
}

#[tokio::test]
async fn c6_sitemap_and_robots_use_the_apex_origin() {
    let xml = page("/sitemap.xml").await;
    let l = locs(&xml);
    assert!(!l.is_empty());
    for u in &l {
        assert!(u.starts_with("https://netray.info/"), "loc off the apex: {u}");
    }
    for p in ["/tools", "/api/v1/docs", "/inspect/email"] {
        assert!(l.contains(&format!("https://netray.info{p}")), "missing {p}");
    }
    let robots = page("/robots.txt").await;
    assert!(robots.lines().any(|x| x == "Sitemap: https://netray.info/sitemap.xml"));
}

#[tokio::test]
async fn c7_scoring_guide_links_the_inspectors_on_the_apex() {
    let html = page("/guide/scoring").await;
    for t in ["ip", "dns", "tls", "http"] {
        assert!(html.contains(&format!("href=\"/inspect/{t}")), "no link to /inspect/{t}");
    }
    assert_eq!(subdomain_urls(&html), Vec::<String>::new());
}

#[tokio::test]
async fn c8_dkim_guide_form_posts_a_domain_to_the_email_inspector() {
    let html = page("/guide/dkim").await;
    let start = html
        .find(r#"action="/inspect/email""#)
        .expect("a form with action=/inspect/email");
    let end = html[start..].find("</form>").expect("form closed") + start;
    assert!(html[start..end].contains(r#"name="d""#), "no input name=d in the form");
}

#[tokio::test]
async fn c9_guide_index_links_guides_apex_relative() {
    let html = page("/guide/").await;
    for g in guides::GUIDES {
        assert!(html.contains(&format!("href=\"/guide/{}\"", g.slug)), "{}", g.slug);
    }
    assert!(!html.contains("href=\"https://netray.info/guide/"));
}
