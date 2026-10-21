//! Contract tests for the one page frame (V2 spec v2-pages, requirements 9 and 32,
//! criteria C2, C12 and C26): every page of the composed router carries the frame's
//! head, header, footer and scripts exactly once, in order, and the 404 and 500 pages share it.
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages;
use std::path::{Path, PathBuf};
use tower::ServiceExt;

const TAGLINE: &str = "Every check of a domain, in one spectrum.";

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .route("/robots.txt", get(pages::robots))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn body_of(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn get_page(uri: &str) -> (StatusCode, String) {
    let resp = app()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    (status, body_of(resp).await)
}

fn count(html: &str, needle: &str) -> usize {
    html.matches(needle).count()
}

fn once(page: &str, html: &str, needle: &str) {
    assert_eq!(
        count(html, needle),
        1,
        "{page}: expected exactly one `{needle}`"
    );
}

fn pos(page: &str, html: &str, needle: &str) -> usize {
    html.find(needle)
        .unwrap_or_else(|| panic!("{page}: missing `{needle}`"))
}

/// The text between `open` and the next `close` (both included).
fn slice<'a>(page: &str, html: &'a str, open: &str, close: &str) -> &'a str {
    let start = pos(page, html, open);
    let end = start
        + html[start..]
            .find(close)
            .unwrap_or_else(|| panic!("{page}: missing `{close}`"))
        + close.len();
    &html[start..end]
}

fn assert_frame(page: &str, html: &str) {
    let lower = html.to_lowercase();
    let head = lower.trim_start();
    assert!(
        head.starts_with("<!doctype html>"),
        "{page}: must start with the doctype"
    );
    let after = head["<!doctype html>".len()..].trim_start();
    assert!(
        after.starts_with("<html lang=\"en\">"),
        "{page}: `<html lang=\"en\">` must follow the doctype"
    );

    assert!(html.contains("<meta charset"), "{page}: charset");
    assert!(html.contains("<meta name=\"viewport\""), "{page}: viewport");
    let title = slice(page, html, "<title>", "</title>");
    assert!(
        title.len() > "<title></title>".len()
            && !title["<title>".len()..title.len() - 8].trim().is_empty(),
        "{page}: title must not be empty"
    );
    assert!(
        html.contains("<meta name=\"description\""),
        "{page}: description"
    );
    let canonical = slice(page, html, "<link rel=\"canonical\"", ">");
    assert!(
        canonical.contains("href=\"https://netray.info/"),
        "{page}: canonical must be absolute: {canonical}"
    );
    for og in ["og:title", "og:description", "og:image"] {
        assert!(
            html.contains(&format!("property=\"{og}\"")),
            "{page}: missing {og}"
        );
    }

    once(page, html, "href=\"/static/app.css\"");
    once(page, html, "href=\"/static/favicon.svg\"");
    once(page, html, "<header");
    once(page, html, "<img src=\"/static/logo.svg\" alt=\"netray\">");
    once(page, html, "data-island=\"theme\"");
    once(page, html, "<main");
    once(page, html, "<footer");
    once(
        page,
        html,
        "<script type=\"module\" src=\"/static/islands.js\"></script>",
    );
    once(page, html, "<script type=\"speculationrules\">");

    // Header: logo inside a link to `/`, theme button, nav links.
    let header = slice(page, html, "<header", "</header>");
    let img = pos(page, header, "<img src=\"/static/logo.svg\"");
    let before_img = &header[..img];
    let anchor = before_img
        .rfind("<a ")
        .unwrap_or_else(|| panic!("{page}: logo is not inside a link"));
    assert!(
        before_img[anchor..].contains("href=\"/\"") && !before_img[anchor..].contains("</a>"),
        "{page}: logo must sit inside a link to /"
    );
    let button = &header[..pos(page, header, "data-island=\"theme\"")];
    assert!(
        button
            .rfind("<button")
            .is_some_and(|b| !button[b..].contains('>')),
        "{page}: data-island=\"theme\" must be on a button"
    );
    for href in ["/inspect/dns", "/guide/", "/api/v1/docs", "/status"] {
        once(page, header, &format!("href=\"{href}\""));
    }
    assert!(
        !header.contains("email.netray.info"),
        "{page}: the nav carries no email link"
    );

    // Footer: tagline and links.
    let footer = slice(page, html, "<footer", "</footer>");
    assert!(footer.contains(TAGLINE), "{page}: footer tagline");
    once(page, footer, "href=\"/privacy\"");
    once(page, footer, "href=\"/status\"");
    once(page, footer, "href=\"/compare\"");

    // Order.
    let h = pos(page, html, "<header");
    let m = pos(page, html, "<main");
    let f = pos(page, html, "<footer");
    let i = pos(
        page,
        html,
        "<script type=\"module\" src=\"/static/islands.js\"></script>",
    );
    let s = pos(page, html, "<script type=\"speculationrules\">");
    assert!(h < m && m < f && f < i && i < s, "{page}: element order");
    let rules_end = s + html[s..].find("</script>").unwrap() + "</script>".len();
    let body_end = pos(page, html, "</body>");
    assert!(
        html[rules_end..body_end].trim().is_empty(),
        "{page}: the speculation rules script must be the last element before </body>"
    );
}

#[tokio::test]
async fn pages_carry_the_frame_once() {
    let (status, not_found) = get_page("/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_frame("404 /nope", &not_found);

    let (status, bad) = get_page("/?d=foo").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_frame("400 /?d=foo", &bad);

    let server_error = body_of(pages::server_error()).await;
    assert_frame("500 server_error()", &server_error);

    // v2-entries' privacy page overrides `header` and `scripts` with nothing (requirement 32),
    // so it carries the frame's prefix and footer, compared in
    // `not_found_and_server_error_share_prefix_and_footer`.
}

fn find_named(dir: &Path, name: &str, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_named(&path, name, out);
        } else if path.file_name().is_some_and(|n| n == name) {
            out.push(path);
        }
    }
}

#[test]
fn exactly_one_base_html_under_templates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let mut found = Vec::new();
    find_named(&root, "base.html", &mut found);
    assert_eq!(found.len(), 1, "base.html files: {found:?}");
}

#[tokio::test]
async fn not_found_and_server_error_share_prefix_and_footer() {
    let (_, not_found) = get_page("/nope").await;
    let server_error = body_of(pages::server_error()).await;

    let prefix = |page: &str, html: &str| html[..pos(page, html, "<head")].to_string();
    assert_eq!(
        prefix("404", &not_found),
        prefix("500", &server_error),
        "frame prefix up to <head>"
    );
    assert_eq!(
        slice("404", &not_found, "<footer", "</footer>"),
        slice("500", &server_error, "<footer", "</footer>"),
        "footer markup"
    );

    // Requirement 32: v2-entries' privacy page starts with the same frame and holds the same
    // footer markup.
    use tower::ServiceExt;
    let res = netray_web::entries::privacy_router()
        .oneshot(
            axum::http::Request::get("/privacy")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "privacy page");
    let privacy = body_of(res).await;
    assert_eq!(
        prefix("privacy", &privacy),
        prefix("404", &not_found),
        "privacy: frame prefix up to <head>"
    );
    assert_eq!(
        slice("privacy", &privacy, "<footer", "</footer>"),
        slice("404", &not_found, "<footer", "</footer>"),
        "privacy: footer markup"
    );
}

/// Reader, phase 3: the navigation labels are requirement 9's approved ones.
#[tokio::test]
async fn navigation_labels_are_the_approved_ones() {
    let html = netray_web_frame_body().await;
    for (href, label) in [
        ("/tools", "All tools →"),
        ("/guide/", "Guides"),
        ("/api/v1/docs", "API"),
        ("/status", "Status"),
    ] {
        let at = html
            .find(&format!("href=\"{href}"))
            .unwrap_or_else(|| panic!("no link to {href}"));
        let tail = &html[at..];
        let text = &tail[tail.find('>').unwrap() + 1..tail.find("</a>").unwrap()];
        assert_eq!(text.trim(), label, "label of {href}");
    }
    // The Inspectors entry is the menu's summary (v2-site-nav requirement 5).
    assert!(html.contains(r#"<summary class="nav-menu__button">Inspectors<"#));
}

async fn netray_web_frame_body() -> String {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let res = axum::Router::new()
        .merge(netray_web::pages::router())
        .fallback(netray_web::pages::not_found)
        .oneshot(Request::get("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();
    String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

/// The logo SVG has only a viewBox, so an `<img>` of it has no intrinsic width and collapses
/// in the flex header unless the stylesheet sizes it (screenshot review, phase 4).
#[test]
fn header_logo_is_sized_by_the_stylesheet() {
    let css = include_str!("../../static/pages.css");
    let at = css
        .find(".page-header img")
        .expect("pages.css styles the header logo");
    let body = &css[at..at + css[at..].find('}').unwrap()];
    assert!(
        body.contains("height:"),
        "the header logo has a height: {body}"
    );
    assert!(
        body.contains("width: auto"),
        "the header logo keeps its aspect ratio: {body}"
    );
}
