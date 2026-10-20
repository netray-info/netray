//! The HTTP views.

use askama::Template;
use netray_model::evidence::{Block, Fields, Value};

use super::{ViewContext, cell_with, claim, claim_tree, frame, seconds, tone};
use crate::components::{Cell, FactCell, FactsGridView, Tone, facts_grid};
use crate::evidence::{cell, display_value};

pub(super) fn render(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    if block.id.module() != "http" || block.id.qualifier().is_some() {
        return None;
    }
    match block.id.name() {
        "csp" => csp(block, ctx),
        "hsts" => hsts(block, ctx),
        "server" => server(block, ctx),
        _ => None,
    }
}

/// The display texts of every value of `key` in `fields`, in order.
fn texts(fields: &Fields, key: &str) -> Vec<String> {
    fields
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| display_value(v))
        .collect()
}

struct Directive {
    name: String,
    sources: Vec<String>,
    issues: Vec<String>,
}

#[derive(Template)]
#[template(path = "partials/csp_view.html")]
struct CspView {
    enforced: bool,
    report_only: bool,
    count: Option<String>,
    issues: Vec<String>,
    directives: Vec<Directive>,
    tone: &'static str,
}

/// `http.csp` as its mode, issue count, the issues of missing directives and one row per
/// directive; directive issues in the tone of the `http.csp` check, warn when it is unknown.
fn csp(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let root = claim_tree(
        block,
        &["report_only", "issue_count", "issue"],
        &["directive", "source", "issue"],
    )?;
    let get = |key: &str| root.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let mode = match get("report_only") {
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    };
    let count = get("issue_count").map(|v| match v {
        Value::Int(1) => "1 issue".to_string(),
        Value::Int(n) => format!("{n} issues"),
        v => display_value(v),
    });
    let directives = root
        .children
        .iter()
        .map(|c| Directive {
            name: texts(&c.fields, "directive").join(" "),
            sources: texts(&c.fields, "source"),
            issues: texts(&c.fields, "issue"),
        })
        .collect();
    let body = CspView {
        enforced: mode == Some(false),
        report_only: mode == Some(true),
        count,
        issues: texts(&root.fields, "issue"),
        directives,
        tone: tone(ctx, "http.csp").unwrap_or(Tone::Warn).as_str(),
    }
    .render()
    .ok()?;
    frame(block, "Content-Security-Policy", &body, ctx)
}

const HSTS: [&str; 3] = ["max_age", "include_subdomains", "preload"];

/// `http.hsts` as a facts grid, each cell in the tone of the `http.hsts` check.
fn hsts(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let pairs = claim(block, &HSTS)?;
    let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let max_age = match get("max_age") {
        Some(v @ Value::Int(n)) => cell_with(block, "max_age", v, ctx, seconds(*n)),
        Some(v) => cell(&block.id, "max_age", v, ctx.decorate),
        None => Cell::from("not parsed"),
    };
    let flag = |key: &str| {
        get(key).map_or_else(
            || Cell::from("not set"),
            |v| cell(&block.id, key, v, ctx.decorate),
        )
    };
    let tone = tone(ctx, "http.hsts");
    let cells = [
        ("max-age", max_age),
        ("includeSubDomains", flag("include_subdomains")),
        ("preload", flag("preload")),
    ]
    .into_iter()
    .map(|(label, value)| FactCell {
        label: label.to_string(),
        values: vec![value],
        chips: false,
        tone,
    })
    .collect();
    let body = facts_grid(&FactsGridView { cells }).ok()?;
    frame(block, "Strict-Transport-Security", &body, ctx)
}

const SERVER: [(&str, &str); 7] = [
    ("final_url", "Final URL"),
    ("status_code", "Status"),
    ("http_version", "HTTP version"),
    ("address", "Address"),
    ("org", "Organisation"),
    ("network_type", "Network type"),
    ("error_kind", "Error"),
];

/// `http.server` as a facts grid of the present keys and a note on the inspection's duration.
fn server(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let known: Vec<&str> = SERVER
        .iter()
        .map(|(key, _)| *key)
        .chain(["inspection_ms"])
        .collect();
    let pairs = claim(block, &known)?;
    let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let cells = SERVER
        .iter()
        .filter_map(|(key, label)| {
            get(key).map(|v| FactCell {
                label: label.to_string(),
                values: vec![cell(&block.id, key, v, ctx.decorate)],
                chips: false,
                tone: None,
            })
        })
        .collect();
    let mut body = facts_grid(&FactsGridView { cells }).ok()?;
    if let Some(v) = get("inspection_ms") {
        let took = askama::filters::escape(
            format!("Inspection took {}", display_value(v)),
            askama::filters::Html,
        )
        .ok()?;
        body.push_str(&format!("<p class=\"protocol-view__note\">{took}</p>"));
    }
    frame(block, "Server", &body, ctx)
}
