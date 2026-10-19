//! V2 spec v2-inspectors, Phase 1, C6: every value of an evidence block is HTML-escaped by askama,
//! in each shape where a value appears.
use netray_model::evidence::{Block, BlockId, Node, Record, Shape, Value};
use netray_web::components::evidence_block;
use netray_web::evidence::block_view;

const HOSTILE: &str = "<script>alert(1)</script>";

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

fn block(id: &str, title: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: title.to_string(),
        shape,
    }
}

#[test]
fn hostile_block_values_render_escaped_in_every_shape() {
    let cases = vec![
        (
            "text block",
            block("x.note", "Note", Shape::Text(HOSTILE.into())),
        ),
        (
            "fields value",
            block(
                "x.info",
                "Info",
                Shape::Fields(vec![("name".into(), text(HOSTILE))]),
            ),
        ),
        (
            "fields time value",
            block(
                "x.info",
                "Info",
                Shape::Fields(vec![("when".into(), Value::Time(HOSTILE.into()))]),
            ),
        ),
        (
            "table cell",
            block(
                "x.rows",
                "Rows",
                Shape::Table {
                    key: "name".into(),
                    columns: vec!["name".into(), "note".into()],
                    rows: vec![vec![text("a"), text(HOSTILE)]],
                },
            ),
        ),
        (
            "record data",
            block(
                "x.recs",
                "Records",
                Shape::Records(vec![Record {
                    owner: text("example.com"),
                    rtype: text("TXT"),
                    ttl: Value::Int(300),
                    data: text(HOSTILE),
                }]),
            ),
        ),
        (
            "record owner",
            block(
                "x.recs",
                "Records",
                Shape::Records(vec![Record {
                    owner: text(HOSTILE),
                    rtype: text("TXT"),
                    ttl: Value::Int(300),
                    data: text("v"),
                }]),
            ),
        ),
        (
            "chain item field",
            block(
                "x.chain",
                "Chain",
                Shape::Chain(vec![vec![("subject".into(), text(HOSTILE))]]),
            ),
        ),
        (
            "tree node field",
            block(
                "x.tree",
                "Tree",
                Shape::Tree(Node {
                    fields: vec![("name".into(), text("root"))],
                    children: vec![Node {
                        fields: vec![("name".into(), text(HOSTILE))],
                        children: vec![],
                    }],
                }),
            ),
        ),
        (
            "title",
            block("x.note", HOSTILE, Shape::Text("plain".into())),
        ),
    ];
    for (name, b) in cases {
        let html = evidence_block(&block_view(&b)).unwrap();
        assert!(!html.contains("<script"), "{name}: raw <script> in: {html}");
        assert!(
            html.contains("&lt;script&gt;") || html.contains("&#60;script&#62;"),
            "{name}: no escaped <script> in: {html}"
        );
    }
}
