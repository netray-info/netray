//! The generic evidence renderer: a `netray-model` block becomes the view `evidence_block`
//! renders. One match on the shape; no module, block or key is named here.

use netray_model::evidence::{Block, Fields, Node, Shape, Value};

use crate::components::{
    EvidenceBlockView, EvidenceShape, Field, RecordRow, TableColumn, TreeItem,
};

/// The display text of a value: text and time verbatim, a duration in milliseconds.
pub fn format_value(value: &Value) -> String {
    match value {
        Value::Text(s) | Value::Time(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Duration(n) => format!("{n} ms"),
        Value::Addr(addr) => addr.to_string(),
    }
}

fn fields(pairs: &Fields) -> Vec<Field> {
    pairs
        .iter()
        .map(|(key, value)| Field {
            key: key.clone(),
            value: format_value(value),
        })
        .collect()
}

fn flatten(node: &Node, depth: u8, out: &mut Vec<TreeItem>) {
    let text = node
        .fields
        .iter()
        .map(|(key, value)| format!("{key}: {}", format_value(value)))
        .collect::<Vec<_>>()
        .join(", ");
    out.push(TreeItem { depth, text });
    for child in &node.children {
        flatten(child, depth.saturating_add(1), out);
    }
}

/// The view of a block; `feeds` is left empty for the caller.
pub fn block_view(block: &Block) -> EvidenceBlockView {
    let shape = match &block.shape {
        Shape::Records(records) => EvidenceShape::Records(
            records
                .iter()
                .map(|r| RecordRow {
                    owner: format_value(&r.owner),
                    rtype: format_value(&r.rtype),
                    ttl: format_value(&r.ttl),
                    data: format_value(&r.data),
                })
                .collect(),
        ),
        Shape::Table { key, columns, rows } => EvidenceShape::Table {
            columns: columns
                .iter()
                .map(|column| TableColumn {
                    name: column.clone(),
                    key: column == key,
                })
                .collect(),
            rows: rows
                .iter()
                .map(|row| row.iter().map(format_value).collect())
                .collect(),
        },
        Shape::Chain(links) => EvidenceShape::Chain(links.iter().map(fields).collect()),
        Shape::Tree(root) => {
            let mut items = Vec::new();
            flatten(root, 0, &mut items);
            EvidenceShape::Tree(items)
        }
        Shape::Fields(pairs) => EvidenceShape::Fields(fields(pairs)),
        Shape::Text(text) => EvidenceShape::Text(text.clone()),
    };
    EvidenceBlockView {
        id: format!("ev-{}", block.id),
        title: block.title.clone(),
        feeds: Vec::new(),
        shape,
    }
}
