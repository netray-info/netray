//! v2-http-ui Phase 3 (C3, C9, C10): the http.isolation view.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::pages::views::{ViewContext, render};

fn block(fields: Vec<(&str, &str)>) -> Block {
    Block {
        id: BlockId::parse("http.isolation").unwrap(),
        title: "Cross-origin isolation".into(),
        shape: Shape::Fields(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), Value::Text(v.to_string())))
                .collect(),
        ),
    }
}

fn render_with(block: &Block, coop: Option<Status>) -> Option<String> {
    let statuses: Vec<(CheckId, Status)> = coop
        .into_iter()
        .map(|s| (CheckId::parse("http.coop").unwrap(), s))
        .collect();
    let none = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &none,
        feeds: &[],
    };
    render(block, &ctx)
}

/// The `facts-grid__cell` element whose label is `label`, up to the next cell or the grid end.
fn cell<'a>(html: &'a str, label: &str) -> &'a str {
    let at = html
        .find(&format!(">{label}<"))
        .unwrap_or_else(|| panic!("{label} missing: {html}"));
    let start = html[..at]
        .rfind("<div class=\"facts-grid__cell")
        .expect("cell start");
    let rest = &html[start + 1..];
    let end = rest
        .find("<div class=\"facts-grid__cell")
        .unwrap_or(rest.len());
    &rest[..end]
}

const COOP: &str = "Cross-Origin-Opener-Policy";
const COEP: &str = "Cross-Origin-Embedder-Policy";
const CORP: &str = "Cross-Origin-Resource-Policy";

fn only_coop() -> Block {
    block(vec![
        ("coop", "same-origin"),
        ("missing", "cross-origin-embedder-policy"),
        ("missing", "cross-origin-resource-policy"),
    ])
}

#[test]
fn c9_coop_carries_the_check_tone_and_coep_corp_read_missing_without_tone() {
    let html = render_with(&only_coop(), Some(Status::Pass)).expect("claimed");
    let coop = cell(&html, COOP);
    assert!(coop.contains("same-origin"), "{coop}");
    assert!(coop.contains(r#"data-tone="pass""#), "{coop}");
    for label in [COEP, CORP] {
        let c = cell(&html, label);
        assert!(c.contains("missing"), "{label}: {c}");
        assert!(!c.contains("data-tone"), "{label}: {c}");
    }
    assert!(
        html.contains(
            r#"<p class="protocol-view__note">COEP and CORP are not part of the grade.</p>"#
        ),
        "{html}"
    );
}

#[test]
fn c9_without_a_coop_status_the_cell_has_no_tone() {
    let html = render_with(&only_coop(), None).expect("claimed");
    assert!(!cell(&html, COOP).contains("data-tone"), "{html}");
}

#[test]
fn c9_a_missing_coop_with_a_warn_check_reads_missing_and_warn() {
    let b = block(vec![("missing", "cross-origin-opener-policy")]);
    let html = render_with(&b, Some(Status::Warn)).expect("claimed");
    let coop = cell(&html, COOP);
    assert!(coop.contains("missing"), "{coop}");
    assert!(coop.contains(r#"data-tone="warn""#), "{coop}");
}

#[test]
fn c10_a_received_corp_shows_its_value_and_present() {
    let b = block(vec![
        ("coop", "same-origin"),
        ("coep", "require-corp"),
        ("corp", "same-site"),
    ]);
    let html = render_with(&b, Some(Status::Pass)).expect("claimed");
    let corp = cell(&html, CORP);
    assert!(corp.contains("same-site"), "{corp}");
    assert!(corp.contains("present"), "{corp}");
}

#[test]
fn c10_a_block_with_an_unknown_key_is_not_claimed() {
    let b = block(vec![("coop", "same-origin"), ("rp", "x")]);
    assert_eq!(render_with(&b, None), None);
}

#[test]
fn a_header_both_received_and_missing_is_not_claimed() {
    use netray_model::evidence::{Block, BlockId, Shape, Value};
    use netray_web::pages::views::{ViewContext, render};
    let block = Block {
        id: BlockId::parse("http.isolation").unwrap(),
        title: "Cross-origin isolation".into(),
        shape: Shape::Fields(vec![
            ("coop".into(), Value::Text("same-origin".into())),
            (
                "missing".into(),
                Value::Text("cross-origin-opener-policy".into()),
            ),
        ]),
    };
    let ctx = ViewContext {
        statuses: &[],
        decorate: &|_, _, _| None,
        feeds: &[],
    };
    assert!(render(&block, &ctx).is_none());
}

#[test]
fn c19_an_unreadable_coep_reads_unreadable_without_tone() {
    let b = block(vec![
        ("coop", "same-origin"),
        ("unreadable", "cross-origin-embedder-policy"),
        ("missing", "cross-origin-resource-policy"),
    ]);
    let html = render_with(&b, Some(Status::Pass)).expect("claimed");
    let coep = cell(&html, COEP);
    assert!(coep.contains("unreadable"), "{coep}");
    assert!(!coep.contains("data-tone"), "{coep}");
}

#[test]
fn c19_an_unreadable_coop_reads_unreadable_in_the_check_tone() {
    let b = block(vec![
        ("unreadable", "cross-origin-opener-policy"),
        ("missing", "cross-origin-embedder-policy"),
        ("missing", "cross-origin-resource-policy"),
    ]);
    let html = render_with(&b, Some(Status::Warn)).expect("claimed");
    let coop = cell(&html, COOP);
    assert!(coop.contains("unreadable"), "{coop}");
    assert!(coop.contains(r#"data-tone="warn""#), "{coop}");
}

#[test]
fn c20_a_header_both_missing_and_unreadable_is_not_claimed() {
    let b = block(vec![
        ("missing", "cross-origin-opener-policy"),
        ("unreadable", "cross-origin-opener-policy"),
    ]);
    assert_eq!(render_with(&b, None), None);
}

#[test]
fn c20_an_unknown_unreadable_name_is_not_claimed() {
    let b = block(vec![("coop", "same-origin"), ("unreadable", "x-unknown")]);
    assert_eq!(render_with(&b, None), None);
}

#[test]
fn c20_a_header_both_received_and_unreadable_is_not_claimed() {
    let b = block(vec![
        ("coop", "same-origin"),
        ("unreadable", "cross-origin-opener-policy"),
    ]);
    assert_eq!(render_with(&b, None), None);
}
