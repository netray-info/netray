//! Final review of v2-inspector-shell (requirements 19, 25): Markdown never turns evidence text
//! into a list or an entity, and an evidence view offers the re-check form `r` submits.
use netray_model::evidence::{Block, BlockId, Shape};
use netray_web::export::{ExportInput, markdown};

fn text_block(text: &str) -> Block {
    Block {
        id: BlockId::parse("email.mta_sts_policy").unwrap(),
        title: "Policy".into(),
        shape: Shape::Text(text.into()),
    }
}

#[test]
fn markdown_text_never_starts_a_list_or_an_entity() {
    for (text, want) in [
        ("- mode: enforce", r"\- mode: enforce"),
        ("+ x", r"\+ x"),
        ("1. first", r"1\. first"),
        ("&lt;b&gt;", r"\&lt;b\&gt;"),
        ("\n- mode: enforce", r"\- mode: enforce"),
        ("   1. first", r"1\. first"),
    ] {
        let blocks = [text_block(text)];
        let md = markdown(&ExportInput {
            target: "example.com",
            title: &[],
            band: None,
            blocks: &blocks,
            errors: &[],
        });
        assert!(md.contains(want), "{text}: {md}");
    }
}
