//! The `/status` page skeleton and the absence of `/privacy` from the pages router
//! (v2-pages requirements 25 and 26, Phase 5 criteria C3 status half, C9, C11).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages;
use tower::ServiceExt;

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn fetch(router: Router, uri: &str) -> (StatusCode, String) {
    let res = router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn main_of(body: &str) -> &str {
    let start = body.find("<main").expect("page has a <main");
    let end = body[start..].find("</main>").expect("page closes </main>") + start;
    &body[start..end]
}

/// The public status page URL V1 names in its noscript block.
fn v1_public_status_url() -> String {
    let path = format!(
        "{}/tests/fixtures/v1-site/status/index.html",
        env!("CARGO_MANIFEST_DIR")
    );
    let v1 = std::fs::read_to_string(path).expect("read site/status/index.html");
    let ns = &v1[v1.find("<noscript>").expect("V1 noscript")..];
    let ns = &ns[..ns.find("</noscript>").unwrap()];
    let href = ns.find("href=\"").expect("V1 noscript link") + 6;
    let rest = &ns[href..];
    rest[..rest.find('"').unwrap()].to_string()
}

/// True when the text holds "of <digits>" or "<digits> monitor(s)/service(s)".
fn has_count_phrase(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    for pair in words.windows(2) {
        let digits = |w: &str| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit());
        if pair[0] == "of" && digits(pair[1]) {
            return Some(pair.join(" "));
        }
        if digits(pair[0]) && matches!(pair[1], "monitor" | "monitors" | "service" | "services") {
            return Some(pair.join(" "));
        }
    }
    None
}

#[tokio::test]
async fn status_page_is_a_skeleton_with_a_noscript_link_and_an_island() {
    let (status, body) = fetch(app(), "/status").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"<html lang="en">"#), "frame");
    assert!(body.contains("<h1"), "heading");

    let main = main_of(&body);
    assert!(main.contains("<p"), "intro paragraph in main");
    assert_eq!(has_count_phrase(main), None, "no monitor count in main");
    assert!(!main.contains("of 7"), "no hard-coded 'of 7'");

    let url = v1_public_status_url();
    assert!(url.starts_with("https://"), "V1 url: {url}");
    let ns_start = body.find("<noscript").expect("a <noscript> element");
    let ns = &body[ns_start..ns_start + body[ns_start..].find("</noscript>").unwrap()];
    assert!(
        ns.contains(&format!(r#"href="{url}""#)),
        "noscript links {url}: {ns}"
    );

    assert!(body.contains(r#"data-island="status""#), "status island");
    assert!(
        main.contains("<template"),
        "server-written templates in main"
    );

    let mut rest = body.as_str();
    while let Some(i) = rest.find("<script") {
        let tag_end = rest[i..].find('>').unwrap() + i;
        let tag = &rest[i..tag_end];
        assert!(
            tag.contains("src=") || tag.contains("speculationrules"),
            "inline script other than speculation rules: {tag}"
        );
        rest = &rest[tag_end..];
    }
}

#[tokio::test]
async fn pages_router_alone_does_not_match_privacy() {
    let (status, body) = fetch(pages::router(), "/privacy").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.contains("<html"), "no page body: {body}");
}
