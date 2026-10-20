//! V2 spec v2-result-fidelity, Phase 6, requirements 36 and 40 (criteria C9 and C12): at phone
//! width (`@media (max-width: 480px)`) the header collapses to wordmark, Theme button and a
//! Menu button, and every link and button of the frame and result page is 44 px tall.
//!
//! Markup conflict, resolved here: the spec puts the nav links inside `details.page-menu` and
//! the Theme button outside it so it stays visible beside the closed menu. `fidelity_header.rs`
//! (R2) still pins the Theme button as the last child of `<nav class="page-nav">`; that pin
//! must be relaxed to "Theme is the header's last element" when this phase lands.
use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages;
use std::collections::HashMap;
use tower::ServiceExt;

const SUMMARY: &str = r#"<summary class="page-menu__button" aria-label="Menu"><span class="page-menu__bar"></span><span class="page-menu__bar"></span><span class="page-menu__bar"></span></summary>"#;
const THEME: &str = r#"<button type="button" class="theme-toggle" data-island="theme" aria-label="Switch theme">Theme</button>"#;

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

/// The text between `open` and the next `close` (both included).
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

/// Bodies of every `@media (max-width: 480px) { ... }` block in `css` (brace matching).
fn phone_blocks(css: &str) -> Vec<String> {
    let marker = "@media (max-width: 480px)";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = css[from..].find(marker) {
        let open = from + i + css[from + i..].find('{').expect("media block opens");
        let mut depth = 0;
        let mut end = css.len();
        for (k, c) in css[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + k;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(css[open + 1..end].to_string());
        from = end;
    }
    out
}

/// Declarations per selector inside the phone media blocks of all static stylesheets and the
/// generated tokens; later rules override earlier ones per property.
fn phone_rules() -> HashMap<String, HashMap<String, String>> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
    let mut sheets: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "css"))
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    sheets.push(netray_web::tokens::CSS.to_string());
    let mut rules: HashMap<String, HashMap<String, String>> = HashMap::new();
    for body in sheets.iter().flat_map(|s| phone_blocks(s)) {
        let mut rest = body.as_str();
        while let Some(o) = rest.find('{') {
            let selectors = rest[..o].trim().to_string();
            let c = rest[o..].find('}').expect("rule closes") + o;
            let decls = &rest[o + 1..c];
            for sel in selectors.split(',').map(str::trim) {
                let entry = rules.entry(sel.to_string()).or_default();
                for d in decls.split(';') {
                    if let Some((k, v)) = d.split_once(':') {
                        entry.insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
            }
            rest = &rest[c + 1..];
        }
    }
    rules
}

/// A property of `selector` at phone width, with `var(--x)` resolved through the tokens CSS
/// (phone values inside the media block win over the root value).
fn phone_prop(selector: &str, prop: &str) -> String {
    let rules = phone_rules();
    let raw = rules
        .get(selector)
        .and_then(|r| r.get(prop))
        .unwrap_or_else(|| panic!("no phone rule gives `{selector}` a `{prop}`"))
        .clone();
    resolve(&raw)
}

fn resolve(value: &str) -> String {
    let mut v = value.to_string();
    while let Some(i) = v.find("var(--") {
        let j = i + v[i..].find(')').unwrap();
        let name = &v[i + 4..j];
        let token = tokens_value(name);
        v = format!("{}{}{}", &v[..i], token, &v[j + 1..]);
    }
    v
}

fn tokens_value(name: &str) -> String {
    let css = netray_web::tokens::CSS;
    let needle = format!("{name}:");
    let mut found = None;
    for blk in phone_blocks(css).iter().map(String::as_str).chain([css]) {
        if let Some(i) = blk.find(&needle) {
            let rest = &blk[i + needle.len()..];
            found = Some(rest[..rest.find(';').unwrap()].trim().to_string());
            break;
        }
    }
    found.unwrap_or_else(|| panic!("token `{name}` not generated"))
}

fn desktop_css() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/static/pages.css")).unwrap()
}

#[tokio::test]
async fn header_markup_has_menu_details_with_nav_and_theme_outside() {
    let html = page("/no-such-page").await;
    let header = between(&html, "<header", "</header>");
    let details = between(header, r#"<details class="page-menu">"#, "</details>");

    assert!(details.contains(SUMMARY), "menu button markup: {details}");
    let nav = between(details, r#"<nav class="page-nav">"#, "</nav>");
    let mut at = 0;
    for needle in [
        r#"href="/inspect/dns">Inspectors</a>"#,
        r#"href="/guide/">Guides</a>"#,
        r#"href="/api/v1/docs">API</a>"#,
        r#"href="/status">Status</a>"#,
    ] {
        at += nav[at..]
            .find(needle)
            .unwrap_or_else(|| panic!("nav lacks `{needle}`: {nav}"))
            + needle.len();
    }
    assert!(
        !details.contains(THEME),
        "Theme button must sit outside the menu"
    );
    assert!(header.contains(THEME), "Theme button stays in the header");
    assert!(
        header.rfind(THEME).unwrap() > header.find("</details>").unwrap()
            || header.find(THEME).unwrap() < header.find("<details").unwrap(),
        "Theme button is beside the details, not inside"
    );
}

#[test]
fn desktop_shows_nav_and_hides_menu_button() {
    let css = desktop_css();
    let rule = between(&css, ".page-menu__button {", "}");
    assert!(
        rule.contains("display: none"),
        "desktop hides the button: {rule}"
    );
}

#[test]
fn phone_header_collapses_to_menu_box() {
    let p = |s: &str, k: &str| phone_prop(s, k);
    assert_eq!(p(".page-menu__button", "min-width"), "44px");
    assert_eq!(p(".page-menu__button", "min-height"), "44px");
    assert_eq!(p(".page-menu__button", "border"), "1px solid #e0e3e8");
    assert_eq!(p(".page-menu__button", "border-radius"), "2px");
    assert_ne!(p(".page-menu__button", "display"), "none");
    assert_eq!(p(".page-menu:not([open]) .page-nav", "display"), "none");
    assert_eq!(p(".page-tagline", "display"), "none");
    assert_eq!(p(".page-header img", "height"), "33px");
}

#[test]
fn phone_links_and_buttons_are_44px_tall() {
    for sel in [
        ".page-footer a",
        ".page-nav a",
        ".clean-bar__summary",
        ".check-row__link",
        ".evidence-tabs__tab",
        ".trace__table-link",
        ".grouping-switch__option",
        "#recheck-button",
        "#share",
    ] {
        let h = phone_prop(sel, "min-height");
        assert_eq!(h, "44px", "`{sel}` min-height at phone width");
    }
}
