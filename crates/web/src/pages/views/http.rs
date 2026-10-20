//! The HTTP views.

use netray_model::evidence::{Block, Value};

use super::{ViewContext, cell_with, claim, frame, seconds, tone};
use crate::components::{Cell, FactCell, FactsGridView, facts_grid};
use crate::evidence::cell;

pub(super) fn render(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    if block.id.module() != "http" || block.id.qualifier().is_some() {
        return None;
    }
    match block.id.name() {
        "hsts" => hsts(block, ctx),
        _ => None,
    }
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
