//! V2 spec v2-pages, Phase 4 (requirement 22; criteria C6, C12): `/sitemap.xml` lists the
//! served entries, parses as XML and carries no `lastmod`; the robots handler answers the
//! three lines of `site/robots.txt`.
use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages::{self, guides};
use tower::ServiceExt;

fn app() -> Router {
    Router::new()
        .route("/robots.txt", get(pages::robots))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn get_path(path: &str) -> Response<Body> {
    app()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn body(res: Response<Body>) -> String {
    String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

/// Strict structural parse: declaration, one `urlset`, balanced `<url><loc>..</loc></url>`
/// entries and nothing else. Returns the `<loc>` values in document order.
fn parse_sitemap(xml: &str) -> Vec<String> {
    let rest = xml
        .strip_prefix(r#"<?xml version="1.0" encoding="UTF-8"?>"#)
        .expect("XML declaration first");
    let rest = rest.trim();
    let open = r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#;
    let rest = rest
        .strip_prefix(open)
        .expect("one urlset with the sitemap namespace");
    let inner = rest
        .strip_suffix("</urlset>")
        .expect("urlset closed at the end");
    assert!(!inner.contains("<urlset"), "exactly one urlset");
    let mut locs = Vec::new();
    let mut s = inner.trim();
    while !s.is_empty() {
        s = s.strip_prefix("<url>").expect("a <url> entry").trim_start();
        s = s.strip_prefix("<loc>").expect("<loc> first in <url>");
        let end = s.find("</loc>").expect("<loc> closed");
        let loc = &s[..end];
        assert!(
            !loc.contains('<') && !loc.contains('&'),
            "plain loc text: {loc}"
        );
        locs.push(loc.to_string());
        s = s[end + "</loc>".len()..].trim_start();
        s = s
            .strip_prefix("</url>")
            .expect("<url> closed after <loc>")
            .trim_start();
    }
    locs
}

#[tokio::test]
async fn sitemap_lists_the_served_entries_and_parses() {
    let res = get_path("/sitemap.xml").await;
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("application/xml"), "content-type {ct}");
    assert!(
        res.headers().contains_key(header::CONTENT_SECURITY_POLICY),
        "pages CSP header present"
    );
    let xml = body(res).await;
    assert!(!xml.contains("lastmod"), "no lastmod");
    let mut got = parse_sitemap(&xml);
    let total = got.len();
    got.sort();
    got.dedup();
    assert_eq!(got.len(), total, "no duplicate loc");

    let mut want: Vec<String> = [
        "https://netray.info/",
        "https://netray.info/guide/",
        "https://netray.info/compare",
        "https://netray.info/status",
        "https://netray.info/privacy",
        "https://netray.info/tools",
        "https://netray.info/api/v1/docs",
        "https://netray.info/inspect/dns",
        "https://netray.info/inspect/tls",
        "https://netray.info/inspect/http",
        "https://netray.info/inspect/email",
        "https://netray.info/inspect/ip",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    want.extend(
        guides::GUIDES
            .iter()
            .map(|g| format!("https://netray.info/guide/{}", g.slug)),
    );
    want.sort();
    assert_eq!(got, want);
}

#[tokio::test]
async fn robots_answers_the_content_of_site_robots_txt() {
    let res = get_path("/robots.txt").await;
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("text/plain"), "content-type {ct}");
    let want = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../site/robots.txt"
    ))
    .unwrap();
    assert_eq!(body(res).await, want);
}
