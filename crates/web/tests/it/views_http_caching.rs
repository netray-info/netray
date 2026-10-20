//! v2-http-ui Phase 3 (C1, C5, C6, C7): the http.caching view.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::pages::views::{ViewContext, render};

fn block(fields: Vec<(&str, Value)>) -> Block {
    Block {
        id: BlockId::parse("http.caching").unwrap(),
        title: "Caching".into(),
        shape: Shape::Fields(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        ),
    }
}

fn view(block: &Block) -> Option<String> {
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    render(block, &ctx)
}

/// The HTML of the cell whose label is `label`: from the label to the next cell (or the end).
fn cell<'a>(html: &'a str, label: &str) -> &'a str {
    let at = html
        .find(&format!(">{label}<"))
        .unwrap_or_else(|| panic!("label {label} missing: {html}"));
    let rest = &html[at..];
    match rest[1..].find("facts-grid__cell") {
        Some(end) => &rest[..end + 1],
        None => rest,
    }
}

#[test]
fn c5_a_full_block_fills_every_cell_without_tone() {
    let b = block(vec![
        ("directive", Value::Text("public".into())),
        ("directive", Value::Text("must-revalidate".into())),
        ("max_age", Value::Int(3600)),
        ("etag", Value::Text("W/\"abc\"".into())),
        ("etag_weak", Value::Bool(true)),
        ("last_modified", Value::Time("2024-01-01T00:00:00Z".into())),
        ("vary", Value::Text("Accept-Encoding".into())),
        ("vary", Value::Text("Accept-Language".into())),
        ("age", Value::Int(42)),
    ]);
    let html = view(&b).expect("claimed");
    assert!(
        html.contains(r#"<h3 class="protocol-view__title">Caching</h3>"#),
        "{html}"
    );
    let cc = cell(&html, "Cache-Control");
    let public = cc.find(">public<").expect(&cc);
    let revalidate = cc.find(">must-revalidate<").expect(&cc);
    assert!(public < revalidate, "{cc}");
    assert_eq!(cc.matches("facts-grid__chip").count(), 3, "{cc}");
    assert!(cell(&html, "max-age").contains("1 h (3600 s)"), "{html}");
    let etag = cell(&html, "ETag");
    assert!(etag.contains("W/&#34;abc&#34;"), "{etag}");
    assert!(etag.contains("weak"), "{etag}");
    assert!(
        cell(&html, "Last-Modified")
            .contains(r#"<time datetime="2024-01-01T00:00:00Z">2024-01-01 00:00 UTC</time>"#),
        "{html}"
    );
    let vary = cell(&html, "Vary");
    assert!(vary.contains(">Accept-Encoding<"), "{vary}");
    assert!(vary.contains(">Accept-Language<"), "{vary}");
    assert_eq!(vary.matches("facts-grid__chip").count(), 2, "{vary}");
    assert!(cell(&html, "Age").contains("42 s"), "{html}");
    assert!(!html.contains("data-tone"), "{html}");
}

#[test]
fn c6_absent_headers_read_not_set_and_a_strong_etag_has_no_weak_note() {
    let b = block(vec![
        ("etag", Value::Text("\"x\"".into())),
        ("etag_weak", Value::Bool(false)),
    ]);
    let html = view(&b).expect("claimed");
    for label in ["Last-Modified", "Vary", "Age"] {
        assert!(cell(&html, label).contains("not set"), "{label}: {html}");
    }
    assert!(!html.contains("weak"), "{html}");
}

#[test]
fn c7_an_empty_block_says_no_caching_headers_and_has_no_grid() {
    let html = view(&block(vec![])).expect("claimed");
    assert!(
        html.contains(r#"<p class="protocol-view__empty">No caching headers recorded</p>"#),
        "{html}"
    );
    assert!(!html.contains("facts-grid"), "{html}");
}

#[test]
fn c1_a_block_with_an_unknown_key_is_not_claimed() {
    let b = block(vec![
        ("max_age", Value::Int(3600)),
        ("surrogate", Value::Text("x".into())),
    ]);
    assert_eq!(view(&b), None);
}

#[test]
fn age_reads_in_the_largest_whole_unit_and_max_age_alone_proves_cache_control() {
    use netray_model::evidence::{Block, BlockId, Shape, Value};
    use netray_web::pages::views::{ViewContext, render};
    let block = Block {
        id: BlockId::parse("http.caching").unwrap(),
        title: "Caching".into(),
        shape: Shape::Fields(vec![
            ("max_age".into(), Value::Int(600)),
            ("age".into(), Value::Int(7200)),
        ]),
    };
    let ctx = ViewContext {
        statuses: &[],
        decorate: &|_, _, _| None,
        feeds: &[],
    };
    let html = render(&block, &ctx).expect("claimed");
    assert!(html.contains("2 h (7200 s)"), "{html}");
    let cc = &html[html.find("Cache-Control").unwrap()..];
    let cc = &cc[..cc.find("facts-grid__cell").unwrap_or(cc.len())];
    assert!(
        cc.contains(r#"<span class="facts-grid__chip">max-age=600</span>"#),
        "{cc}"
    );
    assert!(
        !cc.contains("not set") && !cc.contains("none recorded"),
        "{cc}"
    );
}
