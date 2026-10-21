//! The DNS views: the part view over `dns.answers` and `dns.servers` (spec v2-dns-ui,
//! requirements 2 to 7) and the block views of `dns.trace` and `dns.dnssec_chain`
//! (requirements 14 and 15).

use std::collections::BTreeSet;

use askama::Template;
use netray_model::evidence::{Block, BlockId, Fields, Node, Shape, Value};

use super::{PartView, ViewContext, bar_width, frame};

use crate::evidence::{Badge, Decorate, Decoration, display_value};

pub(super) fn render(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    match block.id.to_string().as_str() {
        "dns.trace" => render_trace(block, ctx),
        "dns.dnssec_chain" => render_dnssec(block, ctx),
        _ => None,
    }
}

/// Whether every key of `fields` is in `known`.
fn only(fields: &Fields, known: &[&str]) -> bool {
    fields.iter().all(|(k, _)| known.contains(&k.as_str()))
}

struct HopServer {
    outcome: String,
    outcome_text: String,
    name: Option<String>,
    address: Option<String>,
    href: Option<String>,
    badges: Vec<Badge>,
    bar: Option<u8>,
    time: Option<String>,
    error: Option<String>,
    referral: Option<String>,
    answers: Vec<String>,
}

struct Hop {
    level: i64,
    zone: Option<String>,
    disagree: bool,
    servers: Vec<HopServer>,
}

#[derive(Template)]
#[template(path = "partials/dns_trace_view.html")]
struct TraceView {
    meta: String,
    hops: Vec<Hop>,
}

/// The delegation trace: a hop per zone level, its servers with latency bars, referrals,
/// answers and errors.
fn render_trace(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let Shape::Tree(root) = &block.shape else {
        return None;
    };
    let claimed = only(&root.fields, &["hops"])
        && root.children.iter().all(|hop| {
            only(
                &hop.fields,
                &["level", "zone", "servers_queried", "referral_groups"],
            ) && hop.children.iter().all(|s| {
                s.children.is_empty()
                    && only(
                        &s.fields,
                        &[
                            "address",
                            "latency",
                            "outcome",
                            "server_name",
                            "error",
                            "referral",
                            "answer",
                        ],
                    )
            })
        });
    if !claimed {
        return None;
    }
    let slowest = root
        .children
        .iter()
        .flat_map(|hop| hop.children.iter())
        .filter_map(|s| duration(&s.fields, "latency"))
        .max()
        .unwrap_or(0);
    let hops = root
        .children
        .iter()
        .map(|hop| Hop {
            level: int(&hop.fields, "level"),
            zone: text(&hop.fields, "zone"),
            disagree: int(&hop.fields, "referral_groups") > 1,
            servers: hop
                .children
                .iter()
                .map(|s| {
                    let f = &s.fields;
                    let address = f.iter().find(|(k, _)| k == "address");
                    let Decoration { href, badges } = address
                        .and_then(|(k, v)| (ctx.decorate)(&block.id, k, v))
                        .unwrap_or_default();
                    let latency = duration(f, "latency");
                    let referral = texts(f, "referral");
                    let answers = texts(f, "answer");
                    let outcome = text(f, "outcome").unwrap_or_default();
                    // The outcome in words: an authoritative NODATA or NXDOMAIN is an answer
                    // without records.
                    let outcome_text = match outcome.as_str() {
                        "answer" if answers.is_empty() => "answer, no records".to_string(),
                        other => other.to_string(),
                    };
                    HopServer {
                        outcome_text,
                        outcome,
                        name: text(f, "server_name"),
                        address: address.map(|(_, v)| display_value(v)),
                        href,
                        badges,
                        bar: latency
                            .filter(|_| slowest > 0)
                            .map(|ms| bar_width(ms, slowest)),
                        time: latency.map(|ms| display_value(&Value::Duration(ms))),
                        error: text(f, "error"),
                        referral: (!referral.is_empty())
                            .then(|| format!("Referral: {}", referral.join(", "))),
                        answers,
                    }
                })
                .collect(),
        })
        .collect();
    let body = TraceView {
        meta: counted(root.children.len() as i64, "hop", "hops"),
        hops,
    }
    .render()
    .ok()?;
    frame(block, "Delegation trace", &body, ctx)
}

struct Finding {
    tone: Option<&'static str>,
    message: String,
}

struct Level {
    level: i64,
    zone: Option<String>,
    latency: Option<String>,
    badge: Option<(&'static str, String)>,
    findings: Vec<Finding>,
    records: Vec<(&'static str, Vec<String>)>,
}

#[derive(Template)]
#[template(path = "partials/dns_dnssec_view.html")]
struct DnssecView {
    meta: String,
    levels: Vec<Level>,
}

/// The DNSSEC chain: a level per zone with its findings and, folded, its key, DS and signature
/// records.
fn render_dnssec(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let Shape::Tree(root) = &block.shape else {
        return None;
    };
    let claimed = only(&root.fields, &["levels"])
        && root.children.iter().all(|level| {
            only(
                &level.fields,
                &["level", "zone", "latency", "dnskey", "ds", "rrsig"],
            ) && level
                .children
                .iter()
                .all(|f| f.children.is_empty() && only(&f.fields, &["severity", "message"]))
        });
    if !claimed {
        return None;
    }
    let (mut warn_total, mut fail_total) = (0, 0);
    let levels = root
        .children
        .iter()
        .map(|level| {
            let findings: Vec<Finding> = level
                .children
                .iter()
                .map(|f| Finding {
                    tone: match text(&f.fields, "severity").as_deref() {
                        Some("ok") => Some("pass"),
                        Some("warning") => Some("warn"),
                        Some("failed") => Some("fail"),
                        _ => None,
                    },
                    message: text(&f.fields, "message").unwrap_or_default(),
                })
                .collect();
            let count_of = |tone| findings.iter().filter(|f| f.tone == Some(tone)).count();
            let (warn, fail) = (count_of("warn"), count_of("fail"));
            warn_total += warn;
            fail_total += fail;
            Level {
                level: int(&level.fields, "level"),
                zone: text(&level.fields, "zone"),
                latency: duration(&level.fields, "latency")
                    .map(|ms| display_value(&Value::Duration(ms))),
                badge: (warn + fail > 0).then(|| {
                    (
                        if fail > 0 { "fail" } else { "warn" },
                        format!("{warn} warn · {fail} fail"),
                    )
                }),
                findings,
                records: [("DNSKEY", "dnskey"), ("DS", "ds"), ("RRSIG", "rrsig")]
                    .into_iter()
                    .map(|(label, key)| (label, texts(&level.fields, key)))
                    .filter(|(_, records)| !records.is_empty())
                    .collect(),
            }
        })
        .collect();
    let body = DnssecView {
        meta: format!(
            "{} · {warn_total} warn · {fail_total} fail",
            counted(root.children.len() as i64, "level", "levels")
        ),
        levels,
    }
    .render()
    .ok()?;
    frame(block, "DNSSEC chain", &body, ctx)
}

/// The display texts of every value of `key` in `fields`, in order.
fn texts(fields: &Fields, key: &str) -> Vec<String> {
    fields
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| display_value(v))
        .collect()
}

fn text(fields: &Fields, key: &str) -> Option<String> {
    texts(fields, key).into_iter().next()
}

fn int(fields: &Fields, key: &str) -> i64 {
    match fields.iter().find(|(k, _)| k == key) {
        Some((_, Value::Int(n))) => *n,
        _ => 0,
    }
}

fn duration(fields: &Fields, key: &str) -> Option<u64> {
    match fields.iter().find(|(k, _)| k == key) {
        Some((_, Value::Duration(ms))) => Some(*ms),
        _ => None,
    }
}

/// `n` with its noun, the noun plural unless `n` is one.
fn counted(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

struct Record {
    name: Option<String>,
    rtype: Option<String>,
    ttl: Option<i64>,
    data: String,
    human: Option<String>,
    explain: Option<String>,
    serial_age_days: Option<i64>,
}

struct Answer {
    server: String,
    address: Option<String>,
    name: Option<String>,
    transport: Option<String>,
    source: Option<String>,
    outcome: String,
    reason: Option<String>,
    error: Option<String>,
    response_code: Option<String>,
    response_time: Option<u64>,
    records: Vec<Record>,
}

impl Answer {
    fn of(node: &Node) -> Answer {
        let f = &node.fields;
        Answer {
            server: text(f, "server").unwrap_or_default(),
            address: text(f, "address"),
            name: text(f, "name"),
            transport: text(f, "transport"),
            source: text(f, "source"),
            outcome: text(f, "outcome").unwrap_or_default(),
            // The error names a referral or a transport failure; a response code alone is
            // NOERROR for a referral.
            reason: text(f, "error").or_else(|| text(f, "response_code")),
            error: text(f, "error"),
            response_code: text(f, "response_code"),
            response_time: duration(f, "response_time"),
            records: node
                .children
                .iter()
                .map(|r| Record {
                    name: text(&r.fields, "name"),
                    rtype: text(&r.fields, "type"),
                    ttl: r
                        .fields
                        .iter()
                        .find(|(k, _)| k == "ttl")
                        .and_then(|(_, v)| match v {
                            Value::Int(n) => Some(*n),
                            _ => None,
                        }),
                    data: text(&r.fields, "data").unwrap_or_default(),
                    human: text(&r.fields, "human"),
                    explain: text(&r.fields, "explain"),
                    serial_age_days: r.fields.iter().find_map(|(k, v)| match (k.as_str(), v) {
                        ("soa_serial_age_days", Value::Int(n)) => Some(*n),
                        _ => None,
                    }),
                })
                .collect(),
        }
    }

    fn measured(&self) -> bool {
        self.outcome != "unmeasured"
    }

    /// The answer's distinct record data, sorted.
    fn data(&self) -> Vec<&str> {
        self.records
            .iter()
            .map(|r| r.data.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

struct TypeAnswers {
    record_type: String,
    agreement: String,
    answers: Vec<Answer>,
}

impl TypeAnswers {
    fn block_id(&self) -> String {
        format!("dns.{}", self.record_type.to_lowercase())
    }

    /// Every error and response code of the type's unmeasured answers.
    fn reasons(&self) -> BTreeSet<String> {
        self.answers
            .iter()
            .filter(|a| !a.measured())
            .flat_map(|a| [a.error.clone(), a.response_code.clone()])
            .flatten()
            .collect()
    }

    fn data(&self) -> BTreeSet<&str> {
        self.answers
            .iter()
            .flat_map(|a| a.records.iter().map(|r| r.data.as_str()))
            .collect()
    }
}

struct RecordView {
    data: String,
    href: Option<String>,
    badges: Vec<Badge>,
    ttl: Option<String>,
    human: Option<String>,
    explain: Option<String>,
    name: Option<String>,
    rtype: Option<String>,
    serial: Option<String>,
    answered_by: Vec<RecordServer>,
}

/// A server that answered a record: its mhost string, transport and response time.
struct RecordServer {
    server: String,
    key: String,
    transport: Option<String>,
    time: Option<String>,
}

struct ServerView {
    server: String,
    key: String,
    label: String,
    transport: Option<String>,
    source: Option<String>,
    bar: Option<u8>,
    time: Option<String>,
}

struct GroupView {
    outcome: String,
    count: String,
    ttl: Option<String>,
    outcome_text: Option<String>,
    records: Vec<RecordView>,
    servers: Vec<ServerView>,
    /// The group Compact keeps: the first answered group, else the first.
    lead: bool,
}

struct CardView {
    id: Option<String>,
    record_type: String,
    agreement: String,
    rank: usize,
    order: usize,
    tone: &'static str,
    badge: String,
    groups: Vec<GroupView>,
}

struct ServerCard {
    server: String,
    key: String,
    label: String,
    transport: Option<String>,
    source: Option<String>,
    measured: String,
    median: Option<String>,
    unmeasured: i64,
    types: Vec<ServerType>,
}

/// One record type as one server answered it.
struct ServerType {
    record_type: String,
    agreement: String,
    diverge: bool,
    outcome: Option<String>,
    records: Vec<String>,
}

/// A tab badge: its tone and its words.
struct TabBadge {
    tone: &'static str,
    label: String,
}

#[derive(Template)]
#[template(path = "partials/dns_view.html")]
struct DnsView {
    mode: String,
    summary: Option<String>,
    meta: Option<String>,
    notices: Vec<String>,
    warnings: Vec<String>,
    warnings_claimed: bool,
    table_anchors: Vec<String>,
    cards: Vec<CardView>,
    servers: Option<Vec<ServerCard>>,
    results_badges: Vec<TabBadge>,
    servers_badges: Vec<TabBadge>,
}

/// The agreement badge of a card: its tone and its words in `mode`.
fn badge(agreement: &str, mode: &str) -> (&'static str, String) {
    match agreement {
        "agree" => (
            "pass",
            match mode {
                "compare" => "all transports agree",
                "authcompare" => "auth and recursive agree",
                _ => "agree",
            }
            .to_string(),
        ),
        "diverge" => ("warn", "diverge".to_string()),
        "single" => ("muted", "single answer".to_string()),
        "unmeasured" => ("muted", "not measured".to_string()),
        other => ("muted", other.to_string()),
    }
}

/// The words of a group's outcome; `None` for answered records.
fn outcome_text(answer: &Answer) -> Option<String> {
    match answer.outcome.as_str() {
        "no_records" => Some("no records".to_string()),
        "nxdomain" => Some("NXDOMAIN".to_string()),
        "unmeasured" => Some(match &answer.reason {
            Some(reason) => format!("not measured: {reason}"),
            None => "not measured".to_string(),
        }),
        _ => None,
    }
}

/// What answers of one group share: outcome, the reason of an unmeasured one, record data.
type GroupKey<'a> = (&'a str, Option<&'a str>, Vec<&'a str>);

/// The answers of one type grouped by identical outcome, reason and record data, largest first.
fn groups(
    answers: &[Answer],
    slowest: u64,
    id: &BlockId,
    decorate: Decorate<'_>,
    shared: &BTreeSet<String>,
) -> Vec<GroupView> {
    let mut keyed: Vec<(GroupKey<'_>, Vec<&Answer>)> = Vec::new();
    for answer in answers {
        let reason = if answer.measured() {
            None
        } else {
            answer.reason.as_deref()
        };
        let key = (answer.outcome.as_str(), reason, answer.data());
        match keyed.iter_mut().find(|(k, _)| *k == key) {
            Some((_, members)) => members.push(answer),
            None => keyed.push((key, vec![answer])),
        }
    }
    keyed.sort_by_key(|(_, members)| std::cmp::Reverse(members.len()));
    let lead = keyed
        .iter()
        .position(|(_, m)| m[0].outcome == "answered")
        .unwrap_or(0);
    keyed
        .into_iter()
        .enumerate()
        .map(|(index, (_, members))| {
            let first = members[0];
            let ttls: Vec<i64> = members
                .iter()
                .flat_map(|m| m.records.iter().filter_map(|r| r.ttl))
                .collect();
            let ttl = ttl_text(&ttls);
            let mut seen = BTreeSet::new();
            let records = first
                .records
                .iter()
                .filter(|r| seen.insert(r.data.as_str()))
                .map(|r| {
                    let key = format!("data:{}", r.rtype.as_deref().unwrap_or_default());
                    let Decoration { href, badges } =
                        decorate(id, &key, &Value::Text(r.data.clone())).unwrap_or_default();
                    let ttls: Vec<i64> = members
                        .iter()
                        .flat_map(|m| m.records.iter())
                        .filter(|x| x.data == r.data)
                        .filter_map(|x| x.ttl)
                        .collect();
                    let answered_by = members
                        .iter()
                        .filter(|m| m.records.iter().any(|x| x.data == r.data))
                        .map(|m| RecordServer {
                            key: server_key(&m.server, m.source.as_deref(), shared),
                            server: m.server.clone(),
                            transport: m.transport.clone(),
                            time: m
                                .response_time
                                .map(|ms| display_value(&Value::Duration(ms))),
                        })
                        .collect();
                    let serial = r.serial_age_days.zip(r.data.split_whitespace().nth(2)).map(
                        |(days, serial)| match days {
                            // A serial dated after today is a date, but no age.
                            d if d < 0 => format!("serial {serial} · dated in the future"),
                            d => format!("serial {serial} · {} old", counted(d, "day", "days")),
                        },
                    );
                    RecordView {
                        data: r.data.clone(),
                        href,
                        badges,
                        ttl: ttl_human(&ttls),
                        human: r.human.clone().filter(|h| *h != r.data),
                        explain: r.explain.clone(),
                        name: r.name.clone(),
                        rtype: r.rtype.clone(),
                        serial,
                        answered_by,
                    }
                })
                .collect();
            let servers = members
                .iter()
                .map(|m| ServerView {
                    key: server_key(&m.server, m.source.as_deref(), shared),
                    server: m.server.clone(),
                    label: m
                        .name
                        .clone()
                        .or_else(|| m.address.clone())
                        .unwrap_or_else(|| m.server.clone()),
                    transport: m.transport.clone(),
                    source: m.source.clone(),
                    bar: m
                        .response_time
                        .filter(|_| slowest > 0)
                        .map(|ms| bar_width(ms, slowest)),
                    time: m
                        .response_time
                        .map(|ms| display_value(&Value::Duration(ms))),
                })
                .collect();
            GroupView {
                outcome: first.outcome.clone(),
                count: format!("{} / {} servers", members.len(), answers.len()),
                ttl,
                outcome_text: outcome_text(first),
                records,
                servers,
                lead: index == lead,
            }
        })
        .collect()
}

/// The queries sent and whether DNSSEC was requested, from the done event.
fn meta_line(root: &Fields) -> Option<String> {
    let mut parts = Vec::new();
    if root.iter().any(|(k, _)| k == "total_queries") {
        parts.push(counted(
            int(root, "total_queries"),
            "query sent",
            "queries sent",
        ));
    }
    if matches!(
        root.iter().find(|(k, _)| k == "dnssec"),
        Some((_, Value::Bool(true)))
    ) {
        parts.push("DNSSEC requested".to_string());
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// "TTL 300 s", or the range "TTL 60 to 300 s"; `None` without a TTL.
fn ttl_text(ttls: &[i64]) -> Option<String> {
    match (ttls.iter().min(), ttls.iter().max()) {
        (Some(lo), Some(hi)) if lo == hi => Some(format!("TTL {lo} s")),
        (Some(lo), Some(hi)) => Some(format!("TTL {lo} to {hi} s")),
        _ => None,
    }
}

/// Seconds in units: the two largest non-zero of days, h, min, s, then the seconds in brackets
/// ("1 h 5 min (3900 s)"); below a minute just "59 s".
fn human_seconds(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 60 {
        return format!("{secs} s");
    }
    let parts: Vec<String> = [
        (secs / 86_400, "day"),
        (secs % 86_400 / 3600, "h"),
        (secs % 3600 / 60, "min"),
        (secs % 60, "s"),
    ]
    .into_iter()
    .filter(|(n, _)| *n != 0)
    .take(2)
    .map(|(n, unit)| match unit {
        "day" => counted(n, "day", "days"),
        _ => format!("{n} {unit}"),
    })
    .collect();
    format!("{} ({secs} s)", parts.join(" "))
}

/// A record's TTL in units, or the range over the group's servers; `None` without a TTL.
fn ttl_human(ttls: &[i64]) -> Option<String> {
    match (ttls.iter().min(), ttls.iter().max()) {
        (Some(lo), Some(hi)) if lo == hi => Some(human_seconds(*lo)),
        (Some(lo), Some(hi)) => Some(format!("{} to {}", human_seconds(*lo), human_seconds(*hi))),
        _ => None,
    }
}

/// The run summary line: counts, transports or authoritative servers, agreement and duration.
fn summary_line(root: &Fields) -> String {
    let mut parts = Vec::new();
    for (key, one, many) in [
        ("record_types", "type", "types"),
        ("lookups", "lookup", "lookups"),
        ("servers", "server", "servers"),
    ] {
        let n = int(root, key);
        if n != 0 {
            parts.push(counted(n, one, many));
        }
    }
    let via: Vec<String> = texts(root, "transport")
        .into_iter()
        .chain(texts(root, "auth_server"))
        .collect();
    if !via.is_empty() {
        parts.push(via.join(", "));
    }
    parts.push(format!("{} agree", int(root, "agree")));
    parts.push(format!("{} diverge", int(root, "diverge")));
    let single = int(root, "single");
    if single != 0 {
        parts.push(format!("{single} single"));
    }
    let unmeasured = int(root, "unmeasured");
    if unmeasured != 0 {
        parts.push(format!("{unmeasured} not measured"));
    }
    if let Some(ms) = duration(root, "duration") {
        parts.push(display_value(&Value::Duration(ms)));
    }
    parts.join(" · ")
}

/// Whether every record of a compare or authcompare table appears among its type's answers.
fn table_shown(block: &Block, types: &[TypeAnswers]) -> bool {
    let Shape::Table { columns, rows, .. } = &block.shape else {
        return false;
    };
    let col = |name: &str| columns.iter().position(|c| c == name);
    let (Some(ty), Some(records), Some(outcome)) = (col("type"), col("records"), col("outcome"))
    else {
        return false;
    };
    rows.iter().all(|row| {
        let cell = |i: usize| row.get(i).map(display_value).unwrap_or_default();
        let Some(t) = types.iter().find(|t| t.record_type == cell(ty)) else {
            return false;
        };
        match cell(outcome).as_str() {
            "answered" => {}
            "no records" => {
                return t
                    .answers
                    .iter()
                    .any(|a| a.outcome == "no_records" || a.outcome == "nxdomain");
            }
            reason => return t.reasons().contains(reason),
        }
        let data = t.data();
        let records = cell(records);
        data.contains(records.as_str()) || records.split("; ").all(|r| data.contains(r))
    })
}

/// Whether a job block `dns.<type>` is fully shown by the card of its type.
fn type_block_shown(block: &Block, t: &TypeAnswers) -> bool {
    match &block.shape {
        Shape::Records(records) => {
            let data = t.data();
            records
                .iter()
                .all(|r| data.contains(display_value(&r.data).as_str()))
        }
        Shape::Fields(pairs) => {
            let keys: BTreeSet<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
            let reason = pairs
                .iter()
                .find(|(k, _)| k == "reason")
                .map(|(_, v)| display_value(v));
            keys == BTreeSet::from(["outcome", "reason"])
                && reason.is_some_and(|r| t.reasons().contains(r.as_str()))
        }
        _ => false,
    }
}

/// The DNS part view: claims `dns.answers` and the blocks its fragment shows in full.
pub(super) fn render_part(blocks: &[Block], ctx: &ViewContext<'_>) -> Option<PartView> {
    let answers = blocks.iter().find(|b| b.id.to_string() == "dns.answers")?;
    let Shape::Tree(root) = &answers.shape else {
        return None;
    };
    let mode = text(&root.fields, "mode").unwrap_or_default();
    let finished = matches!(
        root.fields.iter().find(|(k, _)| k == "finished"),
        Some((_, Value::Bool(true)))
    );
    let types: Vec<TypeAnswers> = root
        .children
        .iter()
        .map(|t| TypeAnswers {
            record_type: text(&t.fields, "record_type").unwrap_or_default(),
            agreement: text(&t.fields, "agreement").unwrap_or_default(),
            answers: t.children.iter().map(Answer::of).collect(),
        })
        .collect();

    let mut claimed: Vec<BlockId> = Vec::new();
    let mut warnings = Vec::new();
    let mut warnings_claimed = false;
    let mut table_anchors = Vec::new();
    let mut servers = None;
    let mut card_ids = BTreeSet::new();
    for block in blocks {
        let id = block.id.to_string();
        let claim = match id.as_str() {
            "dns.answers" => true,
            "dns.servers" => match &block.shape {
                Shape::Tree(node) => {
                    servers = Some(server_cards(node, &types));
                    true
                }
                _ => false,
            },
            "dns.warnings" => match &block.shape {
                Shape::Table { columns, rows, .. } if columns.len() == 1 => {
                    warnings.extend(rows.iter().filter_map(|r| r.first()).map(display_value));
                    warnings_claimed = true;
                    true
                }
                _ => false,
            },
            "dns.compare" | "dns.authcompare" => {
                let shown = table_shown(block, &types);
                if shown {
                    table_anchors.push(id.clone());
                }
                shown
            }
            _ => match types.iter().find(|t| t.block_id() == id) {
                Some(t) if type_block_shown(block, t) => {
                    card_ids.insert(id.clone());
                    true
                }
                _ => false,
            },
        };
        if claim {
            claimed.push(block.id.clone());
        }
    }

    let slowest = types
        .iter()
        .flat_map(|t| t.answers.iter())
        .filter(|a| a.measured())
        .filter_map(|a| a.response_time)
        .max()
        .unwrap_or(0);
    let mut by_slowest: Vec<(usize, u64)> = types
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let max = t.answers.iter().filter_map(|a| a.response_time).max();
            (i, max.unwrap_or(0))
        })
        .collect();
    by_slowest.sort_by_key(|(_, ms)| std::cmp::Reverse(*ms));
    let shared = shared_servers(&types);
    let cards = types
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let (tone, badge) = badge(&t.agreement, &mode);
            let block_id = t.block_id();
            CardView {
                id: card_ids.contains(&block_id).then_some(block_id),
                record_type: t.record_type.clone(),
                agreement: t.agreement.clone(),
                rank: by_slowest.iter().position(|(j, _)| *j == i).unwrap_or(i) + 1,
                order: i + 1,
                tone,
                badge,
                groups: groups(&t.answers, slowest, &answers.id, ctx.decorate, &shared),
            }
        })
        .collect();

    let mut notices: Vec<String> = texts(&root.fields, "degraded_provider")
        .into_iter()
        .map(|p| format!("{p} skipped: circuit open"))
        .collect();
    if !finished {
        notices.push("The query ended before every server answered".to_string());
    }
    let results_badges = match (
        int(&root.fields, "diverge"),
        int(&root.fields, "unmeasured"),
    ) {
        // A job that never finished is not clean: its missing types are in no count.
        (0, 0) if finished => vec![pass_badge()],
        (0, 0) => Vec::new(),
        (diverge, unmeasured) => [
            (diverge, "warn", "diverge"),
            (unmeasured, "muted", "not measured"),
        ]
        .into_iter()
        .filter(|(n, ..)| *n > 0)
        .map(|(n, tone, words)| TabBadge {
            tone,
            label: format!("{n} {words}"),
        })
        .collect(),
    };
    let servers_badges = servers
        .as_ref()
        .map(
            |cards| match cards.iter().filter(|c| c.unmeasured > 0).count() {
                0 if finished => vec![pass_badge()],
                0 => Vec::new(),
                n => vec![TabBadge {
                    tone: "warn",
                    label: format!("{n} not measured"),
                }],
            },
        )
        .unwrap_or_default();
    let html = DnsView {
        mode,
        summary: finished.then(|| summary_line(&root.fields)),
        meta: meta_line(&root.fields),
        notices,
        warnings,
        warnings_claimed,
        table_anchors,
        cards,
        servers,
        results_badges,
        servers_badges,
    }
    .render()
    .ok()?;
    Some(PartView { html, claimed })
}

/// The key the server filter matches on: the server, after its source in authcompare, where a
/// recursive and an authoritative server can print alike.
fn server_key(server: &str, source: Option<&str>, shared: &BTreeSet<String>) -> String {
    match source {
        Some(source) if shared.contains(server) => format!("{source} {server}"),
        _ => server.to_string(),
    }
}

/// The servers that answer under more than one source.
fn shared_servers(types: &[TypeAnswers]) -> BTreeSet<String> {
    let mut sources: std::collections::BTreeMap<&str, BTreeSet<Option<&str>>> =
        std::collections::BTreeMap::new();
    for a in types.iter().flat_map(|t| t.answers.iter()) {
        sources
            .entry(a.server.as_str())
            .or_default()
            .insert(a.source.as_deref());
    }
    sources
        .into_iter()
        .filter(|(_, s)| s.len() > 1)
        .map(|(server, _)| server.to_string())
        .collect()
}

fn pass_badge() -> TabBadge {
    TabBadge {
        tone: "pass",
        label: "✓".to_string(),
    }
}

/// One card per server summary of `dns.servers`, with that server's answer per record type.
fn server_cards(root: &Node, types: &[TypeAnswers]) -> Vec<ServerCard> {
    let shared = shared_servers(types);
    root.children
        .iter()
        .map(|s| {
            let f = &s.fields;
            let server = text(f, "server").unwrap_or_default();
            let source = text(f, "source");
            let transport = text(f, "transport");
            let unmeasured = int(f, "unmeasured");
            let mut address = None;
            let type_rows = types
                .iter()
                .filter_map(|t| {
                    let answer = t.answers.iter().find(|a| {
                        a.server == server && a.source == source && a.transport == transport
                    })?;
                    address = address.take().or_else(|| answer.address.clone());
                    Some(ServerType {
                        record_type: t.record_type.clone(),
                        agreement: t.agreement.clone(),
                        diverge: t.agreement == "diverge",
                        outcome: outcome_text(answer),
                        records: answer.records.iter().map(|r| r.data.clone()).collect(),
                    })
                })
                .collect();
            ServerCard {
                label: text(f, "name")
                    .or(address)
                    .unwrap_or_else(|| server.clone()),
                key: server_key(&server, source.as_deref(), &shared),
                server,
                transport,
                source,
                measured: format!("{} / {} measured", int(f, "measured"), int(f, "lookups")),
                median: duration(f, "median_response_time")
                    .map(|ms| display_value(&Value::Duration(ms))),
                unmeasured,
                types: type_rows,
            }
        })
        .collect()
}
