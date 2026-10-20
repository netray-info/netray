//! The generic evidence renderer: a `netray-model` block becomes the view `evidence_block`
//! renders. One match on the shape; no module, block or key is named here.

use askama::Template;
use netray_engine::catalogue::catalogue;
use netray_model::CheckResult;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::evidence::{Block, BlockId, Fields, Node, Shape, Value};
use netray_model::run::{Run, RunCheck, SectionKind};
use netray_model::{Grade, Protocol, Status};

use crate::compact::{CompactView, compact};
use crate::components::{
    Cell, CheckRowView, EvidenceBlockView, EvidenceShape, Field, RecordRow, RunLineView,
    TableColumn, TreeItem,
};
use crate::entries::view::{epoch_days_to_ymd, status_tone};
use crate::explore::{ExploreLink, view_link};
use crate::pages::result::views::{line_code, meta_text};

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

/// A short labelled chip beside a value, in a status tone.
#[derive(Debug, Clone, PartialEq)]
pub struct Badge {
    pub label: String,
    pub tone: BadgeTone,
}

/// The tone of a badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeTone {
    Muted,
    Warn,
    Fail,
}

impl BadgeTone {
    /// The `data-tone` value.
    pub fn as_str(self) -> &'static str {
        match self {
            BadgeTone::Muted => "muted",
            BadgeTone::Warn => "warn",
            BadgeTone::Fail => "fail",
        }
    }
}

/// What a value carries beside its text: a link and badges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Decoration {
    pub href: Option<String>,
    pub badges: Vec<Badge>,
}

/// The decoration of a value of a block, by the block, the value's key and the value.
pub type Decorate<'a> = &'a dyn Fn(&BlockId, &str, &Value) -> Option<Decoration>;

fn cell(id: &BlockId, key: &str, value: &Value, decorate: Decorate<'_>) -> Cell {
    let text = format_value(value);
    match decorate(id, key, value) {
        Some(Decoration { href, badges }) => Cell { text, href, badges },
        None => Cell::from(text),
    }
}

fn fields(id: &BlockId, pairs: &Fields, decorate: Decorate<'_>) -> Vec<Field> {
    pairs
        .iter()
        .map(|(key, value)| Field {
            key: key.clone(),
            value: cell(id, key, value, decorate),
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
    block_view_with(block, &|_, _, _| None)
}

/// The view of a block with each value decorated by `decorate`; the key of a record value is
/// its field (`owner`, `rtype`, `ttl`, `data`), of a table value its column, of a field its key.
pub fn block_view_with(block: &Block, decorate: Decorate<'_>) -> EvidenceBlockView {
    let id = &block.id;
    let shape = match &block.shape {
        Shape::Records(records) => EvidenceShape::Records(
            records
                .iter()
                .map(|r| RecordRow {
                    owner: cell(id, "owner", &r.owner, decorate),
                    rtype: cell(id, "rtype", &r.rtype, decorate),
                    ttl: cell(id, "ttl", &r.ttl, decorate),
                    data: cell(
                        id,
                        &format!("data:{}", format_value(&r.rtype)),
                        &r.data,
                        decorate,
                    ),
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
                .map(|row| {
                    columns
                        .iter()
                        .map(String::as_str)
                        .chain(std::iter::repeat(""))
                        .zip(row)
                        .map(|(column, value)| cell(id, column, value, decorate))
                        .collect()
                })
                .collect(),
        },
        Shape::Chain(links) => EvidenceShape::Chain(
            links
                .iter()
                .map(|link| fields(id, link, decorate))
                .collect(),
        ),
        Shape::Tree(root) => {
            let mut items = Vec::new();
            flatten(root, 0, &mut items);
            EvidenceShape::Tree(items)
        }
        Shape::Fields(pairs) => EvidenceShape::Fields(fields(id, pairs, decorate)),
        Shape::Text(text) => EvidenceShape::Text(text.clone()),
    };
    EvidenceBlockView {
        id: format!("ev-{}", block.id),
        title: block.title.clone(),
        feeds: Vec::new(),
        shape,
    }
}

/// The run line of a stored entry: the run's time to the minute, UTC, and the entry id.
pub fn entry_run_line(created_at: i64, entry_id: &str) -> RunLineView {
    let secs = created_at.max(0) as u64;
    let (year, month, day) = epoch_days_to_ymd(secs / 86_400);
    let hour = (secs % 86_400) / 3_600;
    let minute = (secs % 3_600) / 60;
    RunLineView {
        parts: vec![
            format!("run of {year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC"),
            format!("entry {entry_id}"),
        ],
    }
}

/// The run line of a live run that is not stored.
pub fn incomplete_run_line() -> RunLineView {
    RunLineView {
        parts: vec!["incomplete, not recorded".to_string()],
    }
}

/// The run title of a live inspector run started at `now` (Unix seconds): not stored.
pub fn live_run_title(now: i64) -> RunLineView {
    RunLineView {
        parts: vec![
            "live".to_string(),
            format!("{} UTC", crate::pages::result::views::utc_minute(now)),
            "not recorded".to_string(),
        ],
    }
}

/// The run title of a shared inspector run started at `started` (Unix seconds), when known.
pub fn shared_run_title(started: Option<i64>) -> RunLineView {
    let mut parts = vec!["shared".to_string()];
    parts.extend(started.map(|t| format!("{} UTC", crate::pages::result::views::utc_minute(t))));
    parts.push("not recorded".to_string());
    RunLineView { parts }
}

/// The entry-view URL of a protocol, with the anchor of a block when one is given.
pub fn href(entry_id: &str, protocol: Protocol, block: Option<&BlockId>) -> String {
    match block {
        Some(block) => format!("/r/{entry_id}/{}#ev-{block}", protocol.as_str()),
        None => format!("/r/{entry_id}/{}", protocol.as_str()),
    }
}

/// One link from a check row to the block that backs it.
#[derive(Debug, Clone)]
pub struct BlockLink {
    pub label: String,
    pub href: String,
}

/// A block of another module that a check of the view references.
#[derive(Debug, Clone)]
pub struct SharedLine {
    pub title: String,
    pub href: String,
}

/// A check row and the links to its evidence.
#[derive(Debug, Clone)]
pub struct EvidenceRow {
    pub check: CheckRowView,
    pub links: Vec<BlockLink>,
}

/// Everything one protocol's evidence section shows.
#[derive(Debug, Clone)]
pub struct EvidenceView {
    pub protocol: Protocol,
    pub run_line: RunLineView,
    pub reason: Option<String>,
    pub rows: Vec<EvidenceRow>,
    pub blocks: Vec<EvidenceBlockView>,
    pub shared: Vec<SharedLine>,
}

/// The live evidence section of `protocol` (interface I8): one row per catalogue check, the
/// blocks of the protocol, and one line per block of another module that a row references.
/// The section is streamed into the result page, which holds every protocol's section, so a
/// foreign block links `#ev-<block id>` on the same page.
pub fn section(run: &Run, protocol: Protocol, run_line: RunLineView) -> EvidenceView {
    build_section(run, protocol, run_line, |bid| format!("#ev-{bid}"))
}

/// The evidence section of `protocol` for a stored entry. A foreign block links the owner's
/// view of the same entry, `/r/<entry id>/<owner>#ev-<block id>`; a module that is no protocol
/// keeps the same-page link.
pub fn entry_section(
    entry_id: &str,
    created_at: i64,
    run: &Run,
    protocol: Protocol,
) -> EvidenceView {
    build_section(
        run,
        protocol,
        entry_run_line(created_at, entry_id),
        |bid| match owner_protocol(bid) {
            Some(owner) => href(entry_id, owner, Some(bid)),
            None => format!("#ev-{bid}"),
        },
    )
}

/// The check rows of a live run that is not stored: one per catalogue check of `protocol`, built
/// by the stored view's row builder, with every block link on the same page.
pub fn live_rows(checks: &[CheckResult], blocks: &[Block], protocol: Protocol) -> Vec<EvidenceRow> {
    let run = Run {
        profile_version: 0,
        engine_version: String::new(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::Incomplete,
        complete: false,
        sections: Vec::new(),
        checks: checks
            .iter()
            .map(|c| RunCheck {
                id: c.id.clone(),
                status: c.status,
                findings: c.findings.clone(),
                // Only blocks this page shows: a link to a block that is not here leads nowhere.
                evidence: c
                    .evidence
                    .iter()
                    .filter(|bid| blocks.iter().any(|b| &b.id == *bid))
                    .cloned()
                    .collect(),
            })
            .collect(),
        blocks: blocks.to_vec(),
    };
    build_section(&run, protocol, RunLineView { parts: vec![] }, |bid| {
        format!("#ev-{bid}")
    })
    .rows
}

fn owner_protocol(bid: &BlockId) -> Option<Protocol> {
    [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ]
    .into_iter()
    .find(|p| p.as_str() == bid.module())
}

fn build_section(
    run: &Run,
    protocol: Protocol,
    run_line: RunLineView,
    foreign_href: impl Fn(&BlockId) -> String,
) -> EvidenceView {
    let owner = protocol.as_str();
    let title_of = |bid: &BlockId| {
        run.blocks
            .iter()
            .find(|b| &b.id == bid)
            .map_or_else(|| bid.to_string(), |b| b.title.clone())
    };

    let mut rows = Vec::new();
    let mut shared: Vec<SharedLine> = Vec::new();
    let mut feeds: Vec<(&BlockId, String)> = Vec::new();
    for spec in catalogue().entries() {
        let found = run.checks.iter().find(|c| c.id == spec.id);
        let evidence = found.map_or(&[][..], |c| c.evidence.as_slice());
        for bid in evidence.iter().filter(|bid| bid.module() == owner) {
            feeds.push((bid, spec.short.clone()));
        }
        if spec.protocol != protocol {
            continue;
        }
        let status = found.map(|c| c.status);
        let flagged = matches!(status, Some(Status::Warn | Status::Fail));
        let body = match found {
            Some(c) if !c.findings.is_empty() => c.findings.join("; "),
            _ if flagged => spec.finding.clone(),
            _ => String::new(),
        };
        let mut links = Vec::new();
        for bid in evidence {
            let label = title_of(bid);
            let href = if bid.module() == owner {
                format!("#ev-{bid}")
            } else {
                foreign_href(bid)
            };
            if bid.module() != owner && !shared.iter().any(|line| line.href == href) {
                shared.push(SharedLine {
                    title: label.clone(),
                    href: href.clone(),
                });
            }
            links.push(BlockLink { label, href });
        }
        rows.push(EvidenceRow {
            check: CheckRowView {
                code: spec.short.clone(),
                line: line_code(&spec.id),
                tone: status_tone(status),
                title: spec.title.clone(),
                since: None,
                body,
                fix: flagged.then(|| spec.fix_hint.clone()),
                meta: Some(meta_text(spec)),
                evidence_href: links.first().map(|l: &BlockLink| l.href.clone()),
                guide_href: Some(spec.guide.clone()),
            },
            links,
        });
    }

    let decorate = crate::links::decorate(&[]);
    let blocks = run
        .blocks
        .iter()
        .filter(|b| b.id.module() == owner)
        .map(|b| {
            let mut view = block_view_with(b, &decorate);
            for (bid, short) in &feeds {
                if **bid == b.id && !view.feeds.contains(short) {
                    view.feeds.push(short.clone());
                }
            }
            view
        })
        .collect();

    let reason = run
        .sections
        .iter()
        .find(|s| s.protocol == protocol && s.outcome == SectionKind::NotApplicable)
        .and_then(|s| s.reason.clone());

    EvidenceView {
        protocol,
        run_line,
        reason,
        rows,
        blocks,
        shared,
    }
}

/// One line a protocol's blocks feed, "TLS·11 Trust".
#[derive(Debug, Clone)]
pub struct PanelFeed {
    pub code: Option<String>,
    pub short: String,
}

/// What one protocol's tab panel on the result page holds.
#[derive(Debug, Clone)]
pub struct PanelView {
    pub blocks: Vec<EvidenceBlockView>,
    pub compact: Option<CompactView>,
    pub feeds: Vec<PanelFeed>,
    pub all_href: Option<String>,
    pub explore: ExploreLink,
    pub reason: Option<String>,
}

impl PanelView {
    /// Whether the panel holds a table, the only shape the phone's swipe note is for.
    pub fn has_table(&self) -> bool {
        self.compact.is_some()
    }
}

/// The tab panel of `protocol` over `run`: its blocks, the lines they feed, the link to the
/// entry's view (`entry_id` of a stored entry) and the explore-live link of `domain`.
pub fn panel(run: &Run, protocol: Protocol, domain: &str, entry_id: Option<&str>) -> PanelView {
    let owner = protocol.as_str();
    let compact = compact(run, protocol);
    let mut feeds: Vec<PanelFeed> = Vec::new();
    for spec in catalogue().entries() {
        let feeds_owner = run
            .checks
            .iter()
            .find(|c| c.id == spec.id)
            .is_some_and(|c| {
                c.evidence.iter().any(|bid| match &compact {
                    Some(view) => view.sources.contains(bid),
                    None => bid.module() == owner,
                })
            });
        if feeds_owner {
            feeds.push(PanelFeed {
                code: line_code(&spec.id),
                short: spec.short.clone(),
            });
        }
    }
    let reason = run
        .sections
        .iter()
        .find(|s| s.protocol == protocol && s.outcome != SectionKind::Measured)
        .map(|s| match (&s.reason, &s.outcome) {
            (Some(reason), _) => reason.clone(),
            (None, SectionKind::TimedOut) => "timed out, nothing measured".to_string(),
            (None, _) => "incomplete, nothing measured".to_string(),
        });
    let decorate = crate::links::decorate(&[]);
    PanelView {
        blocks: run
            .blocks
            .iter()
            .filter(|b| b.id.module() == owner)
            .map(|b| block_view_with(b, &decorate))
            .collect(),
        compact,
        feeds,
        all_href: entry_id.map(|id| href(id, protocol, None)),
        explore: {
            // The board's wording: the inspector the live view opens in ("Open in the TLS inspector").
            let link = view_link(protocol, domain);
            let tool = link
                .href
                .strip_prefix("/inspect/")
                .and_then(|rest| rest.split('?').next())
                .unwrap_or("")
                .to_uppercase();
            ExploreLink {
                label: format!("Open in the {tool} inspector"),
                ..link
            }
        },
        reason,
    }
}

#[derive(Template)]
#[template(path = "partials/evidence_section.html")]
struct EvidenceSectionTemplate<'a> {
    view: &'a EvidenceView,
}

/// Renders the evidence section of one protocol.
pub fn render_section(view: &EvidenceView) -> askama::Result<String> {
    EvidenceSectionTemplate { view }.render()
}
