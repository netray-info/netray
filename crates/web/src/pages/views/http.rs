//! The HTTP views.

use askama::Template;
use netray_model::evidence::{Block, Fields, Value};

use super::{ViewContext, cell_with, claim, claim_tree, frame, seconds, tone};
use crate::components::{Cell, FactCell, FactsGridView, Tone, facts_grid};
use crate::evidence::{cell, display_value, format_value};

pub(super) fn render(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    if block.id.module() != "http" || block.id.qualifier().is_some() {
        return None;
    }
    match block.id.name() {
        "caching" => caching(block, ctx),
        "cdn" => cdn(block, ctx),
        "csp" => csp(block, ctx),
        "hsts" => hsts(block, ctx),
        "isolation" => isolation(block, ctx),
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

const CACHING: [&str; 7] = [
    "directive",
    "max_age",
    "etag",
    "etag_weak",
    "last_modified",
    "vary",
    "age",
];

/// A cell of `values`, "not set" when there are none.
fn fact(label: &str, values: Vec<Cell>, chips: bool) -> FactCell {
    let chips = chips && !values.is_empty();
    let values = if values.is_empty() {
        vec![Cell::from("not set")]
    } else {
        values
    };
    FactCell {
        label: label.to_string(),
        values,
        chips,
        tone: None,
    }
}

/// Every value of `key` as a cell.
fn cells_of(block: &Block, fields: &Fields, key: &str, ctx: &ViewContext<'_>) -> Vec<Cell> {
    fields
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| cell(&block.id, key, v, ctx.decorate))
        .collect()
}

/// `http.caching` as a facts grid; an empty block as one line.
fn caching(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let pairs = claim(block, &CACHING)?;
    if pairs.is_empty() {
        return frame(
            block,
            "Caching",
            "<p class=\"protocol-view__empty\">No caching headers recorded</p>",
            ctx,
        );
    }
    let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let seconds_cell = |key: &str| {
        get(key)
            .map(|v| match v {
                Value::Int(n) => cell_with(block, key, v, ctx, seconds(*n)),
                v => cell(&block.id, key, v, ctx.decorate),
            })
            .into_iter()
            .collect()
    };
    let mut etag = cells_of(block, pairs, "etag", ctx);
    if !etag.is_empty() && matches!(get("etag_weak"), Some(Value::Bool(true))) {
        etag.push(Cell::from("weak"));
    }
    // The block keeps six named directives and max-age; a max-age alone still proves the header.
    let mut directives = cells_of(block, pairs, "directive", ctx);
    if let Some(max_age) = get("max_age") {
        directives.push(Cell::from(format!("max-age={}", format_value(max_age))));
    }
    let cache_control = if directives.is_empty() {
        FactCell {
            values: vec![Cell::from("none recorded")],
            ..fact("Cache-Control", Vec::new(), false)
        }
    } else {
        fact("Cache-Control", directives, true)
    };
    let cells = vec![
        cache_control,
        fact("max-age", seconds_cell("max_age"), false),
        fact("ETag", etag, false),
        fact(
            "Last-Modified",
            cells_of(block, pairs, "last_modified", ctx),
            false,
        ),
        fact("Vary", cells_of(block, pairs, "vary", ctx), true),
        fact("Age", seconds_cell("age"), false),
    ];
    let body = facts_grid(&FactsGridView { cells }).ok()?;
    frame(block, "Caching", &body, ctx)
}

/// `http.cdn` as a facts grid of the provider, the cache status and the indicators.
fn cdn(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let pairs = claim(block, &["provider", "cache_status", "indicator"])?;
    let present = |label: &str, key: &str, chips: bool| {
        let values = cells_of(block, pairs, key, ctx);
        (!values.is_empty()).then(|| fact(label, values, chips))
    };
    let cells = [
        Some(fact(
            "Provider",
            cells_of(block, pairs, "provider", ctx),
            false,
        )),
        present("Cache status", "cache_status", false),
        present("Indicators", "indicator", true),
    ]
    .into_iter()
    .flatten()
    .collect();
    let body = facts_grid(&FactsGridView { cells }).ok()?;
    frame(block, "CDN", &body, ctx)
}

/// `http.isolation` as three cells, COOP in the tone of the `http.coop` check; COEP and CORP
/// are not graded and carry none.
fn isolation(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let pairs = claim(block, &["coop", "coep", "corp", "missing"])?;
    let listed = texts(pairs, "missing");
    const HEADERS: [(&str, &str); 3] = [
        ("coop", "cross-origin-opener-policy"),
        ("coep", "cross-origin-embedder-policy"),
        ("corp", "cross-origin-resource-policy"),
    ];
    // A block the backend never writes (a header both received and missing, or an unknown name
    // under `missing`) is not claimed: the generic block shows it as stored.
    let known = |name: &String| HEADERS.iter().any(|(_, wire)| wire == name);
    let contradicts = HEADERS.iter().any(|(key, wire)| {
        pairs.iter().any(|(k, _)| k == key) && listed.iter().any(|name| name == wire)
    });
    if contradicts || !listed.iter().all(known) {
        return None;
    }
    let header = |label: &str, key: &str, _wire: &str, tone, mark_present: bool| {
        let mut values = cells_of(block, pairs, key, ctx);
        if values.is_empty() {
            values.push(Cell::from("missing"));
        } else if mark_present {
            values.push(Cell::from("present"));
        }
        FactCell {
            label: label.to_string(),
            values,
            chips: false,
            tone,
        }
    };
    let cells = vec![
        header(
            "Cross-Origin-Opener-Policy",
            "coop",
            "cross-origin-opener-policy",
            tone(ctx, "http.coop"),
            false,
        ),
        header(
            "Cross-Origin-Embedder-Policy",
            "coep",
            "cross-origin-embedder-policy",
            None,
            true,
        ),
        header(
            "Cross-Origin-Resource-Policy",
            "corp",
            "cross-origin-resource-policy",
            None,
            true,
        ),
    ];
    let mut body = facts_grid(&FactsGridView { cells }).ok()?;
    body.push_str("<p class=\"protocol-view__note\">COEP and CORP are not part of the grade.</p>");
    frame(block, "Cross-origin isolation", &body, ctx)
}
