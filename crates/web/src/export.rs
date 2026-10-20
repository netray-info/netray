//! The exports of a finished inspector run (spec v2-inspector-shell, requirements 19 and 20):
//! Markdown, CSV and JSON text built from the target, the run title, the band's checks and the
//! blocks the run streamed.

use netray_engine::catalogue::{CheckSpec, catalogue};
use netray_model::evidence::{Block, Node, Shape};
use netray_model::run::{Run, SectionKind};
use netray_model::{CheckResult, Grade, Protocol, Status};
use serde_json::json;

use crate::entries::view::grade_letter;
use crate::evidence::format_value;
use crate::pages::result::views::line_code;

/// What an export is built from.
pub struct ExportInput<'a> {
    pub target: &'a str,
    pub title: &'a [String],
    pub band: Option<(&'a [Protocol], &'a [CheckResult])>,
    pub blocks: &'a [Block],
    /// Parts or the band that ended without a result: (section, reason). Rule M: a failure is in
    /// every export with its reason, never an empty answer.
    pub errors: &'a [(String, String)],
}

/// The target with every run of characters outside `[A-Za-z0-9.]` replaced by one `-`, trimmed
/// of `-`.
pub fn slug(target: &str) -> String {
    let mut out = String::new();
    for c in target.chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn status_word(status: Status) -> &'static str {
    match status {
        Status::Pass => "pass",
        Status::Warn => "warn",
        Status::Fail => "fail",
        Status::NotApplicable => "not_applicable",
        Status::NotTested => "not_tested",
        Status::Unmeasured => "unmeasured",
    }
}

struct CheckRow {
    id: String,
    code: String,
    title: String,
    status: &'static str,
    finding: String,
    fix: String,
}

fn check_rows(band: Option<(&[Protocol], &[CheckResult])>) -> Vec<CheckRow> {
    let Some((protocols, checks)) = band else {
        return Vec::new();
    };
    catalogue()
        .entries()
        .iter()
        .filter(|spec| protocols.contains(&spec.protocol))
        .map(|spec: &CheckSpec| {
            let result = checks.iter().find(|c| c.id == spec.id);
            let status = result.map_or(Status::NotTested, |r| r.status);
            let bad = matches!(status, Status::Warn | Status::Fail);
            let finding = match result {
                Some(r) if !r.findings.is_empty() => r.findings.join("; "),
                _ if bad => spec.finding.clone(),
                _ => String::new(),
            };
            CheckRow {
                id: spec.id.to_string(),
                code: line_code(&spec.id).unwrap_or_default(),
                title: spec.title.clone(),
                status: status_word(status),
                finding,
                fix: if bad {
                    spec.fix_hint.clone()
                } else {
                    String::new()
                },
            }
        })
        .collect()
}

/// The checks of a stored run as results.
pub fn checks_of(run: &Run) -> Vec<CheckResult> {
    run.checks
        .iter()
        .map(|c| CheckResult {
            id: c.id.clone(),
            status: c.status,
            findings: c.findings.clone(),
            evidence: c.evidence.clone(),
        })
        .collect()
}

/// The sections of `protocols` a stored run ended without a result: (protocol, reason).
pub fn section_errors(run: &Run, protocols: &[Protocol]) -> Vec<(String, String)> {
    run.sections
        .iter()
        .filter(|s| protocols.contains(&s.protocol))
        .filter_map(|s| {
            let fallback = match s.outcome {
                SectionKind::Measured | SectionKind::NotApplicable => return None,
                SectionKind::Incomplete => "incomplete",
                SectionKind::TimedOut => "timed out",
            };
            let reason = s.reason.clone().unwrap_or_else(|| fallback.to_string());
            Some((s.protocol.as_str().to_string(), reason))
        })
        .collect()
}

fn preorder<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(node);
    for child in &node.children {
        preorder(child, out);
    }
}

/// Text that renders as itself in Markdown: every character a renderer reads as markup is
/// backslash-escaped, line breaks become spaces.
fn md_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\r' | '\n' => out.push(' '),
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '<' | '>' | '|' | '~' | '#' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn md_cell(s: &str) -> String {
    md_escape(s)
}

fn md_row(cells: &[String]) -> String {
    let cells: Vec<String> = cells.iter().map(|c| md_cell(c)).collect();
    format!("| {} |\n", cells.join(" | "))
}

fn md_table(header: &[&str], rows: &[Vec<String>]) -> String {
    let head: Vec<String> = header.iter().map(|h| (*h).to_string()).collect();
    let mut out = md_row(&head);
    out.push_str(&format!("|{}\n", "---|".repeat(header.len())));
    for row in rows {
        out.push_str(&md_row(row));
    }
    out
}

fn md_fields(fields: &[(String, netray_model::evidence::Value)]) -> String {
    fields
        .iter()
        .map(|(k, v)| format!("{k}: {}", format_value(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn md_list(items: impl Iterator<Item = String>) -> String {
    items.map(|i| format!("- {}\n", md_escape(&i))).collect()
}

/// The Markdown of a stored entry on its result page: the grade, the score when the page shows
/// one, the open issues and every catalogue check.
pub fn entry_markdown(
    domain: &str,
    grade: Grade,
    score: Option<u8>,
    checks: &[CheckResult],
) -> String {
    let protocols: Vec<Protocol> = catalogue().entries().iter().map(|e| e.protocol).collect();
    let rows = check_rows(Some((&protocols, checks)));
    let score = score.map_or_else(String::new, |n| format!(" · score {n} / 100"));
    let mut out = format!(
        "# {}\n\nGrade {}{score}\n\n## Open issues\n\n",
        md_escape(domain),
        grade_letter(grade)
    );
    let issues: Vec<&CheckRow> = rows
        .iter()
        .filter(|r| matches!(r.status, "warn" | "fail"))
        .collect();
    if issues.is_empty() {
        out.push_str("None\n");
    }
    for r in issues {
        out.push_str(&format!(
            "- {} `{}` {}: {}\n",
            r.status.to_uppercase(),
            r.id,
            md_escape(&r.title),
            md_escape(&r.finding)
        ));
    }
    out.push_str("\n## Checks\n\n");
    out.push_str(&md_table(
        &["Code", "Check", "Status", "Finding", "Fix"],
        &[],
    ));
    for r in &rows {
        let cells: Vec<String> = [
            r.title.as_str(),
            r.status,
            r.finding.as_str(),
            r.fix.as_str(),
        ]
        .iter()
        .map(|c| md_cell(c))
        .collect();
        out.push_str(&format!("| `{}` | {} |\n", r.id, cells.join(" | ")));
    }
    out
}

pub fn markdown(input: &ExportInput<'_>) -> String {
    let mut out = format!("# {}\n\n", md_escape(input.target));
    if !input.title.is_empty() {
        out.push_str(&format!("{}\n\n", input.title.join(" · ")));
    }
    if input.band.is_some() {
        let rows: Vec<Vec<String>> = check_rows(input.band)
            .into_iter()
            .map(|r| vec![r.code, r.title, r.status.to_string(), r.finding, r.fix])
            .collect();
        out.push_str("## Checks\n\n");
        out.push_str(&md_table(
            &["Code", "Check", "Status", "Finding", "Fix"],
            &rows,
        ));
        out.push('\n');
    }
    for block in input.blocks {
        out.push_str(&format!("## {}\n\n", md_escape(&block.title)));
        match &block.shape {
            Shape::Table { columns, rows, .. } => {
                let header: Vec<&str> = columns.iter().map(String::as_str).collect();
                let rows: Vec<Vec<String>> = rows
                    .iter()
                    .map(|r| r.iter().map(format_value).collect())
                    .collect();
                out.push_str(&md_table(&header, &rows));
            }
            Shape::Fields(fields) => {
                let rows: Vec<Vec<String>> = fields
                    .iter()
                    .map(|(k, v)| vec![k.clone(), format_value(v)])
                    .collect();
                out.push_str(&md_table(&["Field", "Value"], &rows));
            }
            Shape::Records(records) => {
                let rows: Vec<Vec<String>> = records
                    .iter()
                    .map(|r| {
                        vec![
                            format_value(&r.owner),
                            format_value(&r.rtype),
                            format_value(&r.ttl),
                            format_value(&r.data),
                        ]
                    })
                    .collect();
                out.push_str(&md_table(&["Owner", "Type", "TTL", "Data"], &rows));
            }
            Shape::Chain(links) => {
                out.push_str(&md_list(links.iter().map(|l| md_fields(l))));
            }
            Shape::Tree(root) => {
                let mut nodes = Vec::new();
                preorder(root, &mut nodes);
                out.push_str(&md_list(nodes.into_iter().map(|n| md_fields(&n.fields))));
            }
            Shape::Text(text) => {
                out.push_str(&md_escape(text));
                out.push('\n');
            }
        }
        out.push('\n');
    }
    for (section, reason) in input.errors {
        out.push_str(&format!(
            "## {}\n\nNot measured: {}\n\n",
            md_escape(section),
            md_escape(reason)
        ));
    }
    out
}

fn csv_field(s: &str) -> String {
    // A value a spreadsheet would read as a formula starts with a quote mark (OWASP CSV injection).
    let guarded;
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        guarded = format!("'{s}");
        guarded.as_str()
    } else {
        s
    };
    if s.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_row(out: &mut String, cells: [&str; 4]) {
    let cells: Vec<String> = cells.iter().map(|c| csv_field(c)).collect();
    out.push_str(&cells.join(","));
    out.push('\n');
}

pub fn csv(input: &ExportInput<'_>) -> String {
    let mut out = String::new();
    csv_row(&mut out, ["section", "item", "field", "value"]);
    for r in check_rows(input.band) {
        csv_row(&mut out, ["checks", &r.id, "status", r.status]);
        csv_row(&mut out, ["checks", &r.id, "finding", &r.finding]);
        csv_row(&mut out, ["checks", &r.id, "fix", &r.fix]);
    }
    for block in input.blocks {
        let id = block.id.to_string();
        let mut row = |item: &str, field: &str, value: &str| {
            csv_row(&mut out, [&id, item, field, value]);
        };
        match &block.shape {
            Shape::Table { key, columns, rows } => {
                let key_at = columns.iter().position(|c| c == key);
                for (n, values) in rows.iter().enumerate() {
                    let item = key_at
                        .and_then(|i| values.get(i))
                        .map_or_else(|| n.to_string(), format_value);
                    for (column, value) in columns.iter().zip(values) {
                        row(&item, column, &format_value(value));
                    }
                }
            }
            Shape::Fields(fields) => {
                for (k, v) in fields {
                    row("", k, &format_value(v));
                }
            }
            Shape::Records(records) => {
                for (n, r) in records.iter().enumerate() {
                    let item = n.to_string();
                    row(&item, "owner", &format_value(&r.owner));
                    row(&item, "type", &format_value(&r.rtype));
                    row(&item, "ttl", &format_value(&r.ttl));
                    row(&item, "data", &format_value(&r.data));
                }
            }
            Shape::Chain(links) => {
                for (n, link) in links.iter().enumerate() {
                    for (k, v) in link {
                        row(&n.to_string(), k, &format_value(v));
                    }
                }
            }
            Shape::Tree(root) => {
                let mut nodes = Vec::new();
                preorder(root, &mut nodes);
                for (n, node) in nodes.into_iter().enumerate() {
                    for (k, v) in &node.fields {
                        row(&n.to_string(), k, &format_value(v));
                    }
                }
            }
            Shape::Text(text) => row("0", "text", text),
        }
    }
    for (section, reason) in input.errors {
        csv_row(&mut out, [section, "", "error", reason]);
    }
    out
}

pub fn json(input: &ExportInput<'_>) -> String {
    let checks = input.band.map_or(&[][..], |(_, checks)| checks);
    let errors: Vec<serde_json::Value> = input
        .errors
        .iter()
        .map(|(section, error)| json!({"section": section, "error": error}))
        .collect();
    let value =
        json!({"target": input.target, "checks": checks, "blocks": input.blocks, "errors": errors});
    serde_json::to_string_pretty(&value).unwrap_or_default()
}
