//! Result fidelity Phase 6, review findings: the Theme button's desktop and phone metrics, the
//! 44 px wordmark, the screen-reader-safe short region name, the menu fallback without
//! `::details-content`, region grades kept in the accessibility tree, and the phone's 12 px
//! controls and tone-wide check-row bar.
use std::collections::HashMap;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages;
use tower::ServiceExt;

type Decls = HashMap<String, String>;

const PHONE: &str = "@media (max-width: 480px)";
const NO_DETAILS_CONTENT: &str = "@supports not selector(::details-content)";

fn strip_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = &rest[i + 2 + rest[i + 2..].find("*/").expect("comment closes") + 2..];
    }
    out.push_str(rest);
    out
}

/// Top-level `(prelude, body)` blocks, brace matched, prelude whitespace-collapsed.
fn blocks(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let mut depth = 0;
        let mut end = rest.len() - 1;
        for (i, c) in rest[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let prelude = rest[..open]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        out.push((prelude, rest[open + 1..end].to_string()));
        rest = &rest[end + 1..];
    }
    out
}

fn decls(body: &str) -> Decls {
    body.split(';')
        .filter_map(|d| d.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// The merged declarations of every rule for exactly `selector` in `context` ("" is the top
/// level, otherwise the exact at-rule prelude).
fn rule(file: &str, context: &str, selector: &str) -> Decls {
    let path = format!("{}/static/{file}", env!("CARGO_MANIFEST_DIR"));
    let css = strip_comments(&std::fs::read_to_string(path).unwrap());
    let scope: Vec<(String, String)> = if context.is_empty() {
        blocks(&css)
            .into_iter()
            .filter(|(p, _)| !p.starts_with('@'))
            .collect()
    } else {
        blocks(&css)
            .into_iter()
            .filter(|(p, _)| p == context)
            .flat_map(|(_, b)| blocks(&b))
            .collect()
    };
    let mut merged = Decls::new();
    for (prelude, body) in scope {
        if prelude.split(',').any(|s| s.trim() == selector) {
            merged.extend(decls(&body));
        }
    }
    assert!(
        !merged.is_empty(),
        "{file}: no `{selector}` rule in `{context}`"
    );
    merged
}

fn prop(d: &Decls, key: &str) -> String {
    d.get(key)
        .unwrap_or_else(|| panic!("no `{key}` in {d:?}"))
        .clone()
}

async fn home() -> String {
    let app = Router::new()
        .route("/", get(pages::query_redirect))
        .merge(pages::router())
        .fallback(pages::not_found);
    let resp = app
        .oneshot(Request::get("/privacy").body(Body::empty()).unwrap())
        .await
        .unwrap();
    String::from_utf8(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn theme_sits_outside_the_nav_and_is_13px_muted_20px_after_it_on_desktop() {
    let body = home().await;
    let nav = &body[body
        .find("<nav class=\"page-nav\" data-island=\"nav-menu\">")
        .unwrap()..];
    let nav = &nav[..nav.find("</nav>").unwrap()];
    assert!(!nav.contains("theme-toggle"), "{nav}");

    let theme = rule("pages.css", "", ".theme-toggle");
    assert_eq!(prop(&theme, "font-size"), "var(--font-size-small)");
    assert_eq!(prop(&theme, "color"), "var(--color-muted)");
    assert_eq!(prop(&theme, "margin-inline-start"), "var(--space-2)");
    let header = rule("pages.css", "", ".page-header");
    assert_eq!(prop(&header, "gap"), "var(--space-3)", "12 + 8 = 20 px");
}

#[test]
fn theme_on_the_phone_is_a_44_px_target_4_px_from_the_menu_button() {
    let theme = rule("pages.css", PHONE, ".theme-toggle");
    assert_eq!(prop(&theme, "padding"), "0 10px");
    assert_eq!(prop(&theme, "min-width"), "44px");
    assert_eq!(prop(&theme, "min-height"), "44px");
    assert_eq!(prop(&theme, "margin-inline"), "0 var(--space-1)");
    assert_eq!(prop(&theme, "order"), "2");
    assert_eq!(prop(&rule("pages.css", PHONE, ".page-menu"), "order"), "3");
    assert_eq!(prop(&rule("pages.css", PHONE, ".page-header"), "gap"), "0");
}

#[test]
fn r40_the_wordmark_link_is_44_px_tall_at_phone_width() {
    let link = rule("pages.css", PHONE, ".page-header > a");
    assert_eq!(prop(&link, "min-height"), "44px");
    assert_eq!(prop(&link, "display"), "flex");
    assert_eq!(prop(&link, "align-items"), "center");
}

#[test]
fn the_phone_short_region_name_is_decoration_for_screen_readers() {
    let after = rule(
        "spectrum.css",
        PHONE,
        ".spectrum__region-name[data-short]::after",
    );
    assert_eq!(prop(&after, "content"), "attr(data-short) / \"\"");
}

#[test]
fn a_browser_without_details_content_keeps_the_menu_button_reachable() {
    let button = rule("pages.css", NO_DETAILS_CONTENT, ".page-menu__button");
    assert_eq!(prop(&button, "display"), "flex");
    assert_eq!(prop(&button, "min-width"), "44px");
    assert_eq!(prop(&button, "min-height"), "44px");
    assert_eq!(
        prop(&rule("pages.css", "", ".page-menu__button"), "display"),
        "none",
        "the desktop base still hides it where ::details-content works"
    );
}

#[test]
fn phone_region_grades_leave_the_view_but_not_the_accessibility_tree() {
    let grade = rule("spectrum.css", PHONE, ".spectrum__region-grade");
    assert_ne!(
        grade.get("display").map(String::as_str),
        Some("none"),
        "{grade:?}"
    );
    assert_ne!(
        grade.get("visibility").map(String::as_str),
        Some("hidden"),
        "{grade:?}"
    );
    assert_eq!(prop(&grade, "position"), "absolute");
    assert_eq!(prop(&grade, "width"), "1px");
    assert_eq!(prop(&grade, "height"), "1px");
    assert_eq!(prop(&grade, "overflow"), "hidden");
    assert_eq!(prop(&grade, "clip-path"), "inset(50%)");
    assert_eq!(
        prop(&rule("pages.css", PHONE, ".legend-row"), "display"),
        "none",
        "board M draws no legend"
    );
}

#[test]
fn phone_hint_in_ink_while_a_line_is_tapped() {
    let hint = rule(
        "spectrum.css",
        PHONE,
        ".spectrum-stepper__hint[data-active]",
    );
    assert_eq!(prop(&hint, "color"), "var(--color-ink)");
}

#[test]
fn phone_grouping_options_and_clean_bar_show_use_the_12px_token() {
    assert!(
        netray_web::tokens::CSS.contains("--font-size-stepper: 12px"),
        "the stepper token is the board's 12 px"
    );
    let option = rule("grouping_switch.css", PHONE, ".grouping-switch__option");
    assert_eq!(prop(&option, "font-size"), "var(--font-size-stepper)");
    let show = rule("pages.css", PHONE, ".clean-bar__show");
    assert_eq!(prop(&show, "font-size"), "var(--font-size-stepper)");
}

#[test]
fn phone_check_row_mark_keeps_the_tone_widths() {
    for (tone, width) in [("fail", "4"), ("warn", "3"), ("pass", "2")] {
        let row = rule(
            "check_row.css",
            PHONE,
            &format!(".check-row[data-tone=\"{tone}\"]"),
        );
        let var = match tone {
            "fail" => "--line-fail",
            "warn" => "--line-warn",
            _ => "--line-pass",
        };
        assert_eq!(
            prop(&row, "border-inline-start-width"),
            format!("var({var})")
        );
        assert!(
            netray_web::tokens::CSS.contains(&format!("{var}: {width}px")),
            "{var} is {width}px"
        );
    }
}
