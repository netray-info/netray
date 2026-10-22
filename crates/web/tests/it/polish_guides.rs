//! v2-ui-polish requirement 11 (C3, C10): the guide page's one article element and its
//! measure, and the phone layout of a guide's tool entry.
use std::fs;
use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_web::pages::{self, guides};
use tower::ServiceExt;

async fn render(slug: &str) -> String {
    let res = pages::router()
        .oneshot(
            Request::get(format!("/guide/{slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "/guide/{slug}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn pages_css() -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static/pages.css")).unwrap()
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
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

/// Bodies of every `@media (max-width: 480px)` block.
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

#[tokio::test]
async fn c10_every_guide_is_one_article_with_h1_first_and_related_last() {
    const OPEN: &str = r#"<article class="guide-article">"#;
    for g in guides::GUIDES {
        let html = render(g.slug).await;
        assert_eq!(
            html.matches(OPEN).count(),
            1,
            "{}: one guide-article",
            g.slug
        );
        let open = html.find(OPEN).unwrap();
        let after = html[open + OPEN.len()..].trim_start();
        assert!(
            after.starts_with("<h1>"),
            "{}: h1 is the first child",
            g.slug
        );
        let related = html
            .find(r#"<section class="guide-related">"#)
            .unwrap_or_else(|| panic!("{}: guide-related missing", g.slug));
        let close = html.rfind("</article>").expect("closing tag");
        let related_end = related + html[related..].find("</section>").unwrap();
        assert!(
            open < related && related_end < close,
            "{}: related inside",
            g.slug
        );
    }
}

#[test]
fn c10_guide_article_measure_is_72ch() {
    let blocks = blocks_in(&pages_css(), ".guide-article");
    assert!(declared(&blocks, "max-width:72ch"), "{blocks:?}");
}

#[test]
fn c3_guide_tool_entry_stacks_on_a_phone() {
    // The gate keeps rules on query_input's classes in its own stylesheet.
    let css = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static/query_input.css"),
    )
    .unwrap();
    let regions = phone_regions(&css);
    let rows = [
        (".guide-tool .query-input__field", "flex-basis:100%"),
        (".guide-tool .query-input__submit", "flex-basis:100%"),
        (".guide-tool .query-input__submit", "min-height:44px"),
    ];
    for (selector, decl) in rows {
        let blocks: Vec<String> = regions
            .iter()
            .flat_map(|r| blocks_in(r, selector))
            .collect();
        assert!(
            declared(&blocks, decl),
            "{selector} lacks {decl} at 480px: {blocks:?}"
        );
    }
}
