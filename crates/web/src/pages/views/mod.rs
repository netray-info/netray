//! Protocol views: readings of named evidence blocks that a page shows in place of the generic
//! evidence block (spec v2-http-ui, requirement 1). One module per protocol; [`render`] asks
//! each in turn. A view claims a block only when it renders every key the block holds.

mod dns;
mod email;
mod http;
mod ip;
mod tls;
pub mod validity;

pub use tls::{tls_band_part, tls_port_view, tls_related_summary, tls_starttls};

pub(crate) use ip::attribution;

use askama::Template;
use netray_model::evidence::{Block, Fields, Node, Shape, Value};
use netray_model::{CheckId, Status};

use crate::components::{Cell, Tone};
use crate::evidence::{Decorate, cell};

/// What a view reads beside its block: the run's check statuses, the value decoration of the
/// page and the short names of the checks the block feeds.
pub struct ViewContext<'a> {
    pub statuses: &'a [(CheckId, Status)],
    pub decorate: Decorate<'a>,
    pub feeds: &'a [String],
}

/// The view fragment of `block` when a protocol view claims it.
pub fn render(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    render_with(block, ctx, &[])
}

/// The view fragment of `block` shown with `siblings` (the block included, in block order); an
/// empty fragment for a block another block's view renders.
pub fn render_with(block: &Block, ctx: &ViewContext<'_>, siblings: &[Block]) -> Option<String> {
    http::render(block, ctx)
        .or_else(|| dns::render(block, ctx))
        .or_else(|| tls::render(block, ctx, siblings))
        .or_else(|| email::render(block, ctx))
        .or_else(|| ip::render(block, ctx))
}

/// Whether a protocol view claims `block` shown with `siblings`.
pub(crate) fn claims(block: &Block, siblings: &[Block]) -> bool {
    let decorate = |_: &_, _: &str, _: &Value| None;
    render_with(
        block,
        &ViewContext {
            statuses: &[],
            decorate: &decorate,
            feeds: &[],
        },
        siblings,
    )
    .is_some()
}

/// The pairs of a fields block whose every key is in `known`.
fn claim<'b>(block: &'b Block, known: &[&str]) -> Option<&'b Fields> {
    match &block.shape {
        Shape::Fields(pairs) if pairs.iter().all(|(k, _)| known.contains(&k.as_str())) => {
            Some(pairs)
        }
        _ => None,
    }
}

/// The root of a tree block whose root keys are all in `root` and whose children are leaves
/// with every key in `child`.
fn claim_tree<'b>(block: &'b Block, root: &[&str], child: &[&str]) -> Option<&'b Node> {
    let known =
        |fields: &Fields, keys: &[&str]| fields.iter().all(|(k, _)| keys.contains(&k.as_str()));
    match &block.shape {
        Shape::Tree(node)
            if known(&node.fields, root)
                && node
                    .children
                    .iter()
                    .all(|c| c.children.is_empty() && known(&c.fields, child)) =>
        {
            Some(node)
        }
        _ => None,
    }
}

/// The tone of check `id` in the run; `None` when the check is unknown or did not judge.
fn tone(ctx: &ViewContext<'_>, id: &str) -> Option<Tone> {
    let id = CheckId::parse(id).ok()?;
    match ctx.statuses.iter().find(|(c, _)| *c == id)?.1 {
        Status::Pass => Some(Tone::Pass),
        Status::Warn => Some(Tone::Warn),
        Status::Fail => Some(Tone::Fail),
        _ => None,
    }
}

/// The evidence cell of the value of `key`, with `text` in place of its display text.
fn cell_with(block: &Block, key: &str, value: &Value, ctx: &ViewContext<'_>, text: String) -> Cell {
    Cell {
        text,
        ..cell(&block.id, key, value, ctx.decorate)
    }
}

/// A count of seconds in its largest whole unit with the seconds after it, "1 day (86400 s)".
fn seconds(n: i64) -> String {
    const UNITS: [(i64, &str, &str); 4] = [
        (31_536_000, "year", "years"),
        (86_400, "day", "days"),
        (3_600, "h", "h"),
        (60, "min", "min"),
    ];
    match UNITS.iter().find(|(size, _, _)| n >= *size) {
        Some((size, one, many)) => {
            let count = n / size;
            let unit = if count == 1 { one } else { many };
            format!("{count} {unit} ({n} s)")
        }
        None => format!("{n} s"),
    }
}

#[derive(Template)]
#[template(path = "partials/protocol_view.html")]
struct Frame<'a> {
    id: String,
    title: &'a str,
    body: &'a str,
    feeds: &'a [String],
}

/// The section a view fragment sits in: the block's anchor, the title, the body and the line
/// the block feeds.
fn frame(block: &Block, title: &str, body: &str, ctx: &ViewContext<'_>) -> Option<String> {
    Frame {
        id: block.id.to_string(),
        title,
        body,
        feeds: ctx.feeds,
    }
    .render()
    .ok()
}
