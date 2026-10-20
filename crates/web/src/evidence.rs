//! The generic evidence renderer: a `netray-model` block becomes the view `evidence_block`
//! renders. One match on the shape; no module, block or key is named here.

use askama::Template;
use netray_engine::catalogue::catalogue;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::evidence::{Block, BlockId, Fields, Node, Shape, Value};
use netray_model::run::{Run, RunCheck, SectionKind};
use netray_model::{CheckId, CheckResult};
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

/// The text a page shows for a value: a duration in ms, s or min and s, a bool as yes or no, a
/// time to the minute in UTC; every other value as [`format_value`].
pub fn display_value(value: &Value) -> String {
    match value {
        Value::Bool(true) => "yes".to_string(),
        Value::Bool(false) => "no".to_string(),
        Value::Duration(ms) => display_duration(*ms),
        Value::Time(t) => display_time(t).unwrap_or_else(|| t.clone()),
        _ => format_value(value),
    }
}

fn display_duration(ms: u64) -> String {
    if ms < 1_000 {
        return format!("{ms} ms");
    }
    if ms < 60_000 {
        // Rounded to a tenth, but never up to "60.0 s": below a minute reads in seconds.
        let tenths = ((ms + 50) / 100).min(599);
        return format!("{}.{} s", tenths / 10, tenths % 10);
    }
    let secs = (ms + 500) / 1_000;
    format!("{} min {} s", secs / 60, secs % 60)
}

/// `YYYY-MM-DD HH:MM UTC` of an RFC 3339 time, its offset applied.
fn display_time(t: &str) -> Option<String> {
    let secs = unix_time(t)?;
    // Back to the civil date in UTC (the inverse algorithm), valid before 1970 too.
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Some(format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rem / 3_600,
        rem % 3_600 / 60
    ))
}

/// The Unix seconds of an RFC 3339 time, to the minute, its offset applied.
pub(crate) fn unix_time(t: &str) -> Option<i64> {
    let num = |range: std::ops::Range<usize>| t.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute) = (num(11..13)?, num(14..16)?);
    let zone = t
        .get(19..)?
        .trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match zone {
        "Z" | "z" => 0,
        _ => {
            let sign = match zone.get(..1)? {
                "+" => 1,
                "-" => -1,
                _ => return None,
            };
            let h = zone.get(1..3)?.parse::<i64>().ok()?;
            let m = zone.get(4..6)?.parse::<i64>().ok()?;
            sign * (h * 3_600 + m * 60)
        }
    };
    // Days from the civil date (H. Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 - offset)
}

/// The key a repeated key `<key>#<n>` (n decimal digits) repeats; `None` for any other key.
/// A caller strips the suffix only when the block also holds the base key: a lone `x#1` is a
/// literal name.
pub(crate) fn repeat_base(key: &str) -> Option<&str> {
    let (base, n) = key.rsplit_once('#')?;
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then_some(base)
}

/// The evidence cell of a value: its display text, its decoration, and for a time the stored
/// value.
pub(crate) fn cell(id: &BlockId, key: &str, value: &Value, decorate: Decorate<'_>) -> Cell {
    let text = display_value(value);
    let datetime = match value {
        Value::Time(t) => Some(t.clone()),
        _ => None,
    };
    let Decoration { href, badges } = decorate(id, key, value).unwrap_or_default();
    Cell {
        text,
        href,
        badges,
        datetime,
        key: None,
    }
}

fn fields(id: &BlockId, pairs: &Fields, decorate: Decorate<'_>) -> Vec<Field> {
    pairs
        .iter()
        .map(|(key, value)| {
            let mut value_cell = cell(id, key, value, decorate);
            let base = match repeat_base(key).filter(|b| pairs.iter().any(|(k, _)| k == b)) {
                Some(base) => {
                    value_cell.key = Some(key.clone());
                    base
                }
                None => key.as_str(),
            };
            let shown = match value {
                Value::Duration(_) => base.strip_suffix("_ms").unwrap_or(base),
                _ => base,
            };
            Field {
                key: shown.to_string(),
                value: value_cell,
            }
        })
        .collect()
}

fn flatten(node: &Node, depth: u8, out: &mut Vec<TreeItem>) {
    let text = node
        .fields
        .iter()
        .map(|(key, value)| format!("{key}: {}", display_value(value)))
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
                    let keys: Vec<String> = columns
                        .iter()
                        .position(|c| c == key)
                        .map(|at| {
                            rows.iter()
                                .filter_map(|r| r.get(at))
                                .map(format_value)
                                .collect()
                        })
                        .unwrap_or_default();
                    columns
                        .iter()
                        .map(String::as_str)
                        .chain(std::iter::repeat(""))
                        .zip(row)
                        .map(|(column, value)| {
                            let mut c = cell(id, column, value, decorate);
                            if column == key
                                && let Some(base) = repeat_base(&c.text)
                                    .filter(|b| keys.iter().any(|k| k == b))
                                    .map(str::to_string)
                            {
                                c.key = Some(std::mem::replace(&mut c.text, base));
                            }
                            c
                        })
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

/// A block as a page shows it: the protocol view when one claims the block, else the generic
/// evidence block.
#[derive(Debug, Clone)]
pub struct BlockSlot {
    pub block: EvidenceBlockView,
    pub view: Option<String>,
}

impl std::ops::Deref for BlockSlot {
    type Target = EvidenceBlockView;

    fn deref(&self) -> &EvidenceBlockView {
        &self.block
    }
}

/// The slot of `block`: its generic view, and the protocol view asked with the run's
/// `statuses`, the checks the block `feeds` and the `siblings` shown with it.
pub(crate) fn slot(
    block: &Block,
    decorate: Decorate<'_>,
    statuses: &[(CheckId, Status)],
    feeds: &[String],
    siblings: &[Block],
) -> BlockSlot {
    let ctx = crate::pages::views::ViewContext {
        statuses,
        decorate,
        feeds,
    };
    BlockSlot {
        block: block_view_with(block, decorate),
        view: crate::pages::views::render_with(block, &ctx, siblings),
    }
}

/// The short names, in catalogue order, of the checks whose evidence lists `bid`; `evidence`
/// gives a check's evidence in the run.
pub(crate) fn block_feeds<'a>(
    evidence: impl Fn(&CheckId) -> Option<&'a [BlockId]>,
    bid: &BlockId,
) -> Vec<String> {
    let mut feeds: Vec<String> = Vec::new();
    for spec in catalogue().entries() {
        if evidence(&spec.id).is_some_and(|e| e.contains(bid)) && !feeds.contains(&spec.short) {
            feeds.push(spec.short.clone());
        }
    }
    feeds
}

fn run_evidence<'a>(run: &'a Run) -> impl Fn(&CheckId) -> Option<&'a [BlockId]> + 'a {
    move |id| {
        run.checks
            .iter()
            .find(|c| &c.id == id)
            .map(|c| c.evidence.as_slice())
    }
}

/// The blocks of module `owner` in the run, in block order.
fn own_blocks(run: &Run, owner: &str) -> Vec<Block> {
    run.blocks
        .iter()
        .filter(|b| b.id.module() == owner)
        .cloned()
        .collect()
}

fn run_statuses(run: &Run) -> Vec<(CheckId, Status)> {
    run.checks
        .iter()
        .map(|c| (c.id.clone(), c.status))
        .collect()
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
    pub blocks: Vec<BlockSlot>,
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
    for spec in catalogue().entries() {
        let found = run.checks.iter().find(|c| c.id == spec.id);
        let evidence = found.map_or(&[][..], |c| c.evidence.as_slice());
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
    let statuses = run_statuses(run);
    let own = own_blocks(run, owner);
    let blocks = own
        .iter()
        .map(|b| {
            let feeds = block_feeds(run_evidence(run), &b.id);
            let mut slot = slot(b, &decorate, &statuses, &feeds, &own);
            slot.block.feeds = feeds;
            slot
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
    pub blocks: Vec<BlockSlot>,
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
    let statuses = run_statuses(run);
    let own = own_blocks(run, owner);
    PanelView {
        blocks: own
            .iter()
            .map(|b| {
                slot(
                    b,
                    &decorate,
                    &statuses,
                    &block_feeds(run_evidence(run), &b.id),
                    &own,
                )
            })
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
