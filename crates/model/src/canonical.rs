//! The canonical form of an evidence block, shared by the run fingerprint and the evidence diff:
//! volatile keys dropped, unordered collections sorted, CSP nonces replaced by one token.

use serde_json::{Value as Json, json};

use crate::evidence::{Block, BlockId, Fields, Node, Shape, Value};

const NONCE_TOKEN: &str = "'nonce-'";

/// A volatile path that names one block: the key or column, and the row key for a table cell.
struct Volatile<'a> {
    key: &'a str,
    row: Option<&'a str>,
}

/// The paths among `paths` that name `id`'s block, whatever its qualifier. Same split as
/// `EvidencePath::parse` in `netray-engine`; a malformed path names nothing.
fn volatile_for<'a>(id: &BlockId, paths: &[&'a str]) -> Vec<Volatile<'a>> {
    paths
        .iter()
        .filter_map(|path| {
            let (block, rest) = path.split_once('/')?;
            let (key, row) = match rest.split_once('@') {
                Some((key, row)) => (key, Some(row)),
                None => (rest, None),
            };
            if key.is_empty() || row.is_some_and(str::is_empty) {
                return None;
            }
            (block.split_once('.') == Some((id.module(), id.name())))
                .then_some(Volatile { key, row })
        })
        .collect()
}

/// Whether a key is dropped wholesale: a path without a row key names it.
fn dropped(volatile: &[Volatile], key: &str) -> bool {
    volatile.iter().any(|p| p.row.is_none() && p.key == key)
}

fn is_csp(key: &str) -> bool {
    key.eq_ignore_ascii_case("content-security-policy")
        || key.eq_ignore_ascii_case("content-security-policy-report-only")
}

/// Replaces every `'nonce-…'` source with [`NONCE_TOKEN`]. A source ends at its closing quote;
/// one met by whitespace or `;` first is unterminated and stays as it is.
fn normalise_nonces(s: &str) -> String {
    const PREFIX: &str = "'nonce-";
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find(PREFIX) {
        let after = &rest[start + PREFIX.len()..];
        let closed = after
            .find(|c: char| c == '\'' || c == ';' || c.is_whitespace())
            .filter(|&end| after[end..].starts_with('\''));
        match closed {
            Some(end) => {
                out.push_str(&rest[..start]);
                out.push_str(NONCE_TOKEN);
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[..start + PREFIX.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn value(csp: bool, v: &Value) -> Json {
    match v {
        Value::Text(s) if csp => json!({ "t": "text", "v": normalise_nonces(s) }),
        _ => serde_json::to_value(v).expect("a typed value serialises"),
    }
}

/// The text a table row is keyed by.
fn key_text(v: &Value) -> String {
    match v {
        Value::Text(s) | Value::Time(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Duration(d) => d.to_string(),
        Value::Addr(a) => a.to_string(),
    }
}

/// `items` ordered by their serialized form.
fn sorted(mut items: Vec<Json>) -> Json {
    items.sort_by_cached_key(Json::to_string);
    Json::Array(items)
}

/// Fields as `[key, value]` pairs, volatile keys dropped, ordered by key then value so that a
/// repeated key has no order.
fn fields(fields: &Fields, volatile: &[Volatile]) -> Json {
    sorted(
        fields
            .iter()
            .filter(|(k, _)| !dropped(volatile, k))
            .map(|(k, v)| json!([k, value(is_csp(k), v)]))
            .collect(),
    )
}

fn node(n: &Node, volatile: &[Volatile]) -> Json {
    json!([
        fields(&n.fields, volatile),
        n.children
            .iter()
            .map(|c| node(c, volatile))
            .collect::<Vec<_>>()
    ])
}

/// A table as `[row key, cells]` pairs ordered by row key, cells ordered by column. A row path
/// drops its whole row, a column path the column.
fn table(key: &str, columns: &[String], rows: &[Vec<Value>], volatile: &[Volatile]) -> Json {
    let key_index = columns.iter().position(|c| c == key);
    let mut out: Vec<(String, Json)> = rows
        .iter()
        .filter_map(|row| {
            let row_key = key_index
                .and_then(|i| row.get(i))
                .map(key_text)
                .unwrap_or_default();
            if volatile.iter().any(|p| p.row == Some(row_key.as_str())) {
                return None;
            }
            let csp = is_csp(&row_key);
            let mut cells: Vec<(&str, Json)> = columns
                .iter()
                .zip(row)
                .filter(|(column, _)| *column == key || !dropped(volatile, column))
                .map(|(column, v)| (column.as_str(), value(csp, v)))
                .collect();
            cells.sort_by(|a, b| a.0.cmp(b.0));
            let cells = cells.into_iter().map(|(c, v)| json!([c, v])).collect();
            Some((row_key, Json::Array(cells)))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Json::Array(
        out.into_iter()
            .map(|(k, cells)| json!([k, cells]))
            .collect(),
    )
}

/// The canonical data of a block's shape; its structure depends on the shape kind.
pub(crate) fn canonical_shape(block: &Block, volatile: &[&str]) -> Json {
    let volatile = volatile_for(&block.id, volatile);
    match &block.shape {
        Shape::Records(records) => sorted(
            records
                .iter()
                .map(|r| {
                    let pairs = [
                        ("owner", &r.owner),
                        ("type", &r.rtype),
                        ("ttl", &r.ttl),
                        ("data", &r.data),
                    ];
                    Json::Array(
                        pairs
                            .into_iter()
                            .filter(|(k, _)| !dropped(&volatile, k))
                            .map(|(k, v)| json!([k, value(false, v)]))
                            .collect(),
                    )
                })
                .collect(),
        ),
        Shape::Table { key, columns, rows } => table(key, columns, rows, &volatile),
        Shape::Chain(items) => Json::Array(items.iter().map(|f| fields(f, &volatile)).collect()),
        Shape::Tree(root) => node(root, &volatile),
        Shape::Fields(f) => fields(f, &volatile),
        Shape::Text(s) => json!(s),
    }
}

/// `[block id, shape kind, canonical shape data]`.
pub(crate) fn canonical_block(block: &Block, volatile: &[&str]) -> Json {
    json!([
        block.id.to_string(),
        block.shape.kind(),
        canonical_shape(block, volatile)
    ])
}
