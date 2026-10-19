//! specs/features/v2-pages, requirement 16: no rendered page, static file or header holds a
//! codename as a whole word (case-insensitive; `-` splits words, so `mhost-prism` hits `prism`).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages;
use tower::ServiceExt;

const WORDS: [&str; 6] = [
    "lens",
    "prism",
    "tlsight",
    "spectra",
    "beacon",
    "ifconfig-rs",
];

/// Every route of the composed router; later phases extend this list.
const ROUTES: &[&str] = &[
    "/nope",
    "/",
    "/?d=foo",
    "/?d=example.com",
    "/robots.txt",
    "/static/app.css",
    "/static/islands.js",
    "/static/logo.svg",
    "/static/favicon.svg",
    "/guide/",
    "/guide",
    "/sitemap.xml",
    "/compare",
    "/status",
];

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .route("/robots.txt", get(pages::robots))
        .merge(pages::router())
        .fallback(pages::not_found)
}

/// The first codename in `text` as a whole word, if any.
fn codename_in(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    for word in WORDS {
        for (at, _) in lower.match_indices(word) {
            let before = lower[..at].chars().next_back();
            let after = lower[at + word.len()..].chars().next();
            if !before.is_some_and(is_word) && !after.is_some_and(is_word) {
                return Some(word);
            }
        }
    }
    None
}

async fn scan(name: &str, response: axum::response::Response) {
    let (parts, body) = response.into_parts();
    for (header, value) in &parts.headers {
        let value = String::from_utf8_lossy(value.as_bytes());
        if let Some(word) = codename_in(&value) {
            panic!("{name}: header {header} holds codename `{word}`: {value}");
        }
    }
    let bytes = body.collect().await.expect("body").to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    if let Some(word) = codename_in(&text) {
        panic!("{name}: body holds codename `{word}`");
    }
}

#[test]
fn codename_rule_is_whole_word() {
    assert_eq!(codename_in("How Lens scores"), Some("lens"));
    assert_eq!(codename_in("mhost-prism"), Some("prism"));
    assert_eq!(codename_in("https://lens.netray.info/"), Some("lens"));
    assert_eq!(codename_in("ifconfig-rs data"), Some("ifconfig-rs"));
    assert_eq!(codename_in("the lensing effect"), None);
    assert_eq!(codename_in("https://dns.netray.info/"), None);
}

#[tokio::test]
async fn no_route_names_a_codename() {
    for route in ROUTES {
        let request = Request::get(*route).body(Body::empty()).expect("request");
        let response = app().oneshot(request).await.expect("response");
        assert_ne!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "{route}"
        );
        scan(route, response).await;
    }
    scan("server_error()", pages::server_error()).await;
    for guide in pages::guides::GUIDES {
        let route = format!("/guide/{}", guide.slug);
        let request = Request::get(&route).body(Body::empty()).expect("request");
        let response = app().oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK, "{route}");
        scan(&route, response).await;
    }
}
