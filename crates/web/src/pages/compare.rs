//! `/compare`: both comparison tables from `compare.toml`, the netray column generated from the
//! check catalogue (v2-pages requirements 23-25).

use std::sync::LazyLock;

use askama::Template;
use axum::http::StatusCode;
use axum::response::Response;
use netray_model::CheckId;
use serde::Deserialize;

use super::{Frame, page};

/// One row of either table: a label, netray's basis and one cell per competitor column.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub label: String,
    pub checks: Option<Vec<String>>,
    pub depth: Option<String>,
    pub product: Option<String>,
    pub note: Option<String>,
    pub cells: Vec<Cell>,
}

/// A symbol and its note.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub symbol: String,
    pub note: String,
}

impl Row {
    /// The netray cell: from the listed checks and their depth, or from the product basis.
    pub fn netray(&self) -> Cell {
        if let Some(checks) = &self.checks {
            if checks.is_empty() {
                return Cell {
                    symbol: "✗".to_string(),
                    note: String::new(),
                };
            }
            let symbol = if self.depth.as_deref() == Some("full") {
                "✓"
            } else {
                "~"
            };
            let note = checks
                .iter()
                .map(|id| title(id).unwrap_or(id.as_str()))
                .collect::<Vec<_>>()
                .join(", ");
            return Cell {
                symbol: symbol.to_string(),
                note,
            };
        }
        let symbol = match self.product.as_deref() {
            Some("yes") => "✓",
            Some("partial") => "~",
            _ => "✗",
        };
        Cell {
            symbol: symbol.to_string(),
            note: self.note.clone().unwrap_or_default(),
        }
    }
}

impl Cell {
    fn kind(&self) -> &'static str {
        match self.symbol.as_str() {
            "✓" => "yes",
            "~" => "part",
            "✗" => "no",
            _ => "oos",
        }
    }

    fn meaning(&self) -> &'static str {
        match self.symbol.as_str() {
            "✓" => "Full coverage",
            "~" => "Partial",
            "✗" => "Not done",
            _ => "Out of scope",
        }
    }
}

fn title(id: &str) -> Option<&'static str> {
    let id = CheckId::parse(id).ok()?;
    netray_engine::catalogue()
        .get(&id)
        .map(|spec| spec.title.as_str())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Doc {
    last_reviewed: String,
    intro: String,
    methodology: String,
    legend: Vec<Legend>,
    honesty: Honesty,
    tables: Vec<Table>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Legend {
    symbol: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Honesty {
    title: String,
    lede: String,
    paragraphs: Vec<Paragraph>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Paragraph {
    lead: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    title: String,
    intro: String,
    groups: Vec<Group>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Group {
    axis: String,
    label_header: String,
    competitors: Vec<Competitor>,
    rows: Vec<Row>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Competitor {
    name: String,
    short: String,
}

static DOC: LazyLock<Doc> = LazyLock::new(|| {
    toml::from_str(include_str!("../../compare.toml")).expect("compare.toml parses")
});

/// Every row of `compare.toml`, both tables and all axes, in document order.
pub fn rows() -> Vec<&'static Row> {
    DOC.tables
        .iter()
        .flat_map(|t| &t.groups)
        .flat_map(|g| &g.rows)
        .collect()
}

/// `"<label>: <id>"` for every listed check id the catalogue does not hold.
pub fn unknown_ids(rows: &[&Row]) -> Vec<String> {
    rows.iter()
        .flat_map(|r| {
            r.checks
                .iter()
                .flatten()
                .filter(|id| title(id).is_none())
                .map(|id| format!("{}: {id}", r.label))
        })
        .collect()
}

struct RowView {
    row: &'static Row,
    netray: Cell,
    notes: Vec<(&'static str, &'static Cell)>,
}

struct GroupView {
    group: &'static Group,
    rows: Vec<RowView>,
}

struct TableView {
    table: &'static Table,
    groups: Vec<GroupView>,
}

#[derive(Template)]
#[template(path = "pages/compare.html")]
struct ComparePage {
    frame: Frame,
    doc: &'static Doc,
    tables: Vec<TableView>,
}

/// `GET /compare`.
pub(super) async fn show() -> Response {
    let doc: &'static Doc = &DOC;
    let tables = doc
        .tables
        .iter()
        .map(|table| TableView {
            table,
            groups: table
                .groups
                .iter()
                .map(|group| GroupView {
                    group,
                    rows: group
                        .rows
                        .iter()
                        .map(|row| RowView {
                            row,
                            netray: row.netray(),
                            notes: group
                                .competitors
                                .iter()
                                .map(|c| c.short.as_str())
                                .zip(row.cells.iter())
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    let frame = Frame::new(
        "Compare - netray",
        "How netray compares with unified domain checkers and per-axis specialists, and where it trades depth for breadth.",
        "/compare",
    );
    page(
        StatusCode::OK,
        "public, max-age=3600",
        &ComparePage { frame, doc, tables },
    )
}
