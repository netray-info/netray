//! V2 spec v2-result-fidelity, requirement 2 (criteria C2 and C27): the page header holds
//! the wordmark link, the tagline and one right-aligned nav, then the unboxed Theme button as the
//! header's last element.
use axum::Router;
use axum::body::Body;
use axum::http::Request;
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

async fn page(uri: &str) -> String {
    let resp = app()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn between<'a>(html: &'a str, open: &str, close: &str) -> &'a str {
    let start = html
        .find(open)
        .unwrap_or_else(|| panic!("missing `{open}`"));
    let end = start
        + html[start..]
            .find(close)
            .unwrap_or_else(|| panic!("missing `{close}`"));
    &html[start..end + close.len()]
}

const THEME: &str = r#"<button type="button" class="theme-toggle" data-island="theme" aria-label="Switch theme">Theme</button>"#;

#[tokio::test]
async fn header_holds_wordmark_tagline_and_nav_ending_in_theme_button() {
    let html = page("/no-such-page").await;
    let header = between(&html, "<header", "</header>");

    assert!(
        header.contains(r#"<span class="page-tagline">your domain, in full spectrum</span>"#),
        "header tagline missing: {header}"
    );
    let nav = between(header, r#"<nav class="page-nav">"#, "</nav>");
    let order = [
        r#"href="/inspect/dns">Inspectors</a>"#,
        r#"href="/guide/">Guides</a>"#,
        r#"href="/api/v1/docs">API</a>"#,
        r#"href="/status">Status</a>"#,
    ];
    let mut at = 0;
    for needle in order {
        let i = nav[at..]
            .find(needle)
            .unwrap_or_else(|| panic!("nav lacks `{needle}` after offset {at}: {nav}"));
        at += i + needle.len();
    }
    assert!(
        header
            .trim_end_matches("</header>")
            .trim_end()
            .ends_with(THEME),
        "Theme button must be the header's last element: {header}"
    );
    assert!(!header.contains("/compare"), "Compare is not in the header");

    let footer = between(&html, "<footer", "</footer>");
    assert!(
        footer.contains(r#"href="/compare""#),
        "footer nav holds Compare"
    );

    // C27: no absolute URL anywhere in the header.
    for bad in [r#"href="http"#, r#"href="//"#] {
        assert!(
            !header.contains(bad),
            "absolute URL `{bad}` in header: {header}"
        );
    }
}

#[test]
fn theme_toggle_is_unboxed_in_pages_css() {
    let css =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/static/pages.css")).unwrap();
    let rule = between(&css, ".theme-toggle {", "}");
    let has = |p: &str| rule.lines().any(|l| l.trim().starts_with(p));
    assert!(
        rule.contains("border: 0") || rule.contains("border: none"),
        "no border: {rule}"
    );
    assert!(
        has("background: transparent"),
        "transparent background: {rule}"
    );
    assert!(has("padding: 0;"), "padding 0: {rule}");
    assert!(has("font: inherit"), "font inherit: {rule}");
}
