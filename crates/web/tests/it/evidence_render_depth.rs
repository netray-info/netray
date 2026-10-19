//! Evidence tree depth (V2 spec v2-inspectors, requirement 2, criteria C2 and C5): the
//! `evidence_block` macro carries no `style` attribute, depth is a clamped `data-depth`
//! styled by one stylesheet rule per depth, and every block carries `id="ev-<block id>"`.
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_web::components::evidence_block;
use netray_web::evidence::block_view;
use std::fs;
use std::path::PathBuf;

fn read(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing file: {}", path.display()))
}

/// A chain of `n` nested nodes: depths 0..n.
fn chain(n: usize) -> Node {
    let mut node = Node {
        fields: vec![("name".to_string(), Value::Text(format!("n{}", n - 1)))],
        children: vec![],
    };
    for i in (0..n - 1).rev() {
        node = Node {
            fields: vec![("name".to_string(), Value::Text(format!("n{i}")))],
            children: vec![node],
        };
    }
    node
}

fn data_depths(html: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("data-depth=\"") {
        rest = &rest[i + 12..];
        let end = rest.find('"').expect("unterminated data-depth");
        out.push(rest[..end].parse().expect("numeric data-depth"));
        rest = &rest[end..];
    }
    out
}

#[test]
fn depth_nine_tree_renders_clamped_data_depth_without_style() {
    let block = Block {
        id: BlockId::parse("dns.zone_tree").unwrap(),
        title: "Zone tree".into(),
        shape: Shape::Tree(chain(10)),
    };
    let html = evidence_block(&block_view(&block)).unwrap();
    assert_eq!(
        data_depths(&html),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 7, 7],
        "in: {html}"
    );
    assert!(!html.contains("style="), "style attribute in: {html}");
    assert!(
        html.contains("id=\"ev-dns.zone_tree\""),
        "block id missing in: {html}"
    );
}

#[test]
fn evidence_block_template_has_no_style_attribute() {
    let template = read("templates/components/evidence_block.html");
    assert!(
        !template.contains("style="),
        "template carries a style attribute"
    );
}

#[test]
fn evidence_block_css_has_one_selector_per_depth() {
    let css = read("static/evidence_block.css");
    for n in 0..=7 {
        let selector = format!("data-depth=\"{n}\"");
        assert!(
            css.contains(&selector),
            "no `{selector}` rule in evidence_block.css"
        );
    }
    assert!(
        !css.contains("--evidence-block-depth"),
        "evidence_block.css still reads --evidence-block-depth"
    );
}
