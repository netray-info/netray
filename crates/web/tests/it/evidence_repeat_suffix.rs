//! v2-http-ui Phase 1, C7/C17/C18: a repeated key `<key>#<n>` shows as `<key>` and keeps the
//! full key in `data-key`; the exports keep the raw key.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::components::evidence_block;
use netray_web::evidence::block_view;
use netray_web::export::{ExportInput, csv, json};

fn text(s: &str) -> Value {
    Value::Text(s.to_owned())
}

fn headers_block() -> Block {
    Block {
        id: BlockId::parse("http.headers").unwrap(),
        title: "Response headers".into(),
        shape: Shape::Table {
            key: "name".into(),
            columns: vec!["name".into(), "value".into()],
            rows: vec![
                vec![text("set-cookie"), text("a=1")],
                vec![text("set-cookie#2"), text("b=2")],
            ],
        },
    }
}

#[test]
fn table_key_cell_drops_the_repeat_suffix_and_keeps_data_key() {
    let html = evidence_block(&block_view(&headers_block())).unwrap();
    assert_eq!(html.matches(">set-cookie<").count(), 2, "in: {html}");
    assert_eq!(html.matches("set-cookie#2").count(), 1, "in: {html}");
    assert!(html.contains("data-key=\"set-cookie#2\""), "in: {html}");
    let second_td = html
        .match_indices("<td")
        .nth(2)
        .map(|(i, _)| &html[i..])
        .unwrap();
    assert!(
        second_td.starts_with("<td data-key=\"set-cookie#2\"")
            || second_td[..second_td.find('>').unwrap()].contains("data-key=\"set-cookie#2\""),
        "second row's key td: {second_td}"
    );
}

#[test]
fn fields_key_drops_the_repeat_suffix_and_non_digits_stay() {
    let block = Block {
        id: BlockId::parse("x.fields").unwrap(),
        title: "Fields".into(),
        shape: Shape::Fields(vec![
            ("indicator".into(), text("a")),
            ("indicator#2".into(), text("b")),
            ("a#b".into(), text("c")),
        ]),
    };
    let html = evidence_block(&block_view(&block)).unwrap();
    assert!(
        html.contains("<dt data-key=\"indicator#2\">indicator</dt>"),
        "in: {html}"
    );
    assert!(html.contains("a#b"), "in: {html}");
    assert!(!html.contains("data-key=\"a#b\""), "in: {html}");
}

#[test]
fn exports_keep_the_raw_key_and_values() {
    let blocks = [headers_block()];
    let title = Vec::new();
    let input = ExportInput {
        target: "example.com",
        title: &title,
        band: None,
        blocks: &blocks,
        errors: &[],
    };
    for out in [json(&input), csv(&input)] {
        assert!(out.contains("set-cookie#2"), "in: {out}");
        assert!(out.contains("a=1") && out.contains("b=2"), "in: {out}");
    }
}

#[test]
fn a_lone_key_with_a_digit_suffix_is_a_literal_name() {
    use netray_model::evidence::{Block, BlockId, Shape, Value};
    use netray_web::components::evidence_block;
    use netray_web::evidence::block_view;
    let block = Block {
        id: BlockId::parse("http.headers").unwrap(),
        title: "Response headers".into(),
        shape: Shape::Table {
            key: "name".into(),
            columns: vec!["name".into(), "value".into()],
            rows: vec![vec![
                Value::Text("x-trace#1".into()),
                Value::Text("abc".into()),
            ]],
        },
    };
    let html = evidence_block(&block_view(&block)).unwrap();
    assert!(html.contains(">x-trace#1<"), "{html}");
    assert!(!html.contains(r#"data-key="x-trace"#), "{html}");
}
