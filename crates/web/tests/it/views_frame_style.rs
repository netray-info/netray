//! v2-http-ui Phase 2 (C14, C4): the protocol view's frame is the board's section heading above
//! a bordered card, and a view ships no script and no island.
use std::fs;
use std::path::PathBuf;

use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render};

fn css() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static/pages.css");
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing file: {}", path.display()))
}

/// The declarations of the rule whose selector is exactly `selector`.
fn rule(css: &str, selector: &str) -> String {
    let open = format!("\n{selector} {{");
    let at = css
        .find(&open)
        .unwrap_or_else(|| panic!("rule {selector} missing in pages.css"));
    let body = &css[at + open.len()..];
    body[..body.find('}').expect("rule closes")].to_string()
}

fn declares(body: &str, property: &str, value: &str) -> bool {
    body.split(';').any(|d| {
        d.split_once(':')
            .is_some_and(|(p, v)| p.trim() == property && v.trim() == value)
    })
}

fn hsts_fragment() -> String {
    let block = Block {
        id: BlockId::parse("http.hsts").unwrap(),
        title: "HSTS".into(),
        shape: Shape::Fields(vec![
            ("max_age".into(), Value::Int(31_536_000)),
            ("include_subdomains".into(), Value::Bool(true)),
            ("preload".into(), Value::Bool(false)),
        ]),
    };
    let statuses = [(CheckId::parse("http.hsts").unwrap(), Status::Pass)];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    render(&block, &ctx).expect("hsts is claimed")
}

#[test]
fn c14_title_is_the_section_heading_above_a_bordered_card() {
    let css = css();
    let title = rule(&css, ".protocol-view__title");
    // The gate refuses a literal font size: the token is 20 px in tokens.json.
    assert!(
        declares(&title, "font-size", "var(--font-size-h2)"),
        "{title}"
    );
    assert!(declares(&title, "font-weight", "650"), "{title}");
    assert!(declares(&title, "letter-spacing", "-0.01em"), "{title}");

    let card = rule(&css, ".protocol-view__card");
    assert!(declares(&card, "padding", "18px"), "{card}");
    assert!(card.contains("border"), "{card}");

    let frame = rule(&css, ".protocol-view");
    assert!(
        !frame.contains("border"),
        "frame must not draw a border: {frame}"
    );
}

#[test]
fn c14_fragment_puts_title_before_the_card_holding_the_grid() {
    let html = hsts_fragment();
    let title = html
        .find("<h3 class=\"protocol-view__title\"")
        .expect("title");
    let card = html
        .find("<div class=\"protocol-view__card\">")
        .unwrap_or_else(|| panic!("card missing in: {html}"));
    let grid = html.find("<div class=\"facts-grid\"").expect("grid");
    assert!(title < card, "title must precede the card: {html}");
    assert!(card < grid, "grid must follow the card opening: {html}");
    assert!(
        !html[card + 1..grid].contains("</div>"),
        "grid must sit directly inside the card: {html}"
    );
}

#[test]
fn c4_fragment_has_no_script_and_no_island() {
    let html = hsts_fragment();
    assert!(!html.contains("<script"), "{html}");
    assert!(!html.contains("data-island"), "{html}");
}

/// The visual reading beside board G: the feeds line sits inside the card, and sections keep
/// G's distance from the card before them.
#[test]
fn the_feeds_line_sits_inside_the_card_and_views_keep_the_section_gap() {
    let block = Block {
        id: BlockId::parse("http.hsts").unwrap(),
        title: "HSTS".into(),
        shape: Shape::Fields(vec![("preload".into(), Value::Bool(false))]),
    };
    let decorate = decorate(&[]);
    let feeds = vec!["HSTS".to_string()];
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &feeds,
    };
    let html = render(&block, &ctx).expect("claimed");
    let card = html.find("protocol-view__card").expect("card");
    let feeds_at = html.find("protocol-view__feeds").expect("feeds");
    let card_end = html.rfind("</div>").expect("card end");
    assert!(card < feeds_at && feeds_at < card_end, "{html}");
    let view = rule(&css(), ".protocol-view");
    assert!(
        declares(&view, "margin-block-start", "var(--space-9)"),
        "{view}"
    );
}
