//! The compact board-style table of one protocol's tab panel: a view over the stored blocks.
//! It reads released keys only and invents no value; an absent key is an empty cell or an
//! absent row.

use netray_model::Protocol;
use netray_model::evidence::{Block, BlockId, Fields, Shape, Value};
use netray_model::run::Run;

use crate::evidence::format_value;

const SEP: &str = " \u{00B7} ";
const MAX_CELL: usize = 64;

/// One cell: the shown text, the full value when the text was cut, and its weight.
#[derive(Debug, Clone)]
pub struct CompactCell {
    pub text: String,
    pub title: Option<String>,
    pub strong: bool,
}

/// The caption, header and rows of one protocol's compact table, and the blocks it reads.
#[derive(Debug, Clone)]
pub struct CompactView {
    pub caption: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<CompactCell>>,
    pub sources: Vec<BlockId>,
}

fn cell(text: impl Into<String>) -> CompactCell {
    let full = text.into();
    if full.chars().count() > MAX_CELL {
        let shown: String = full.chars().take(MAX_CELL - 1).collect();
        return CompactCell {
            text: format!("{shown}…"),
            title: Some(full),
            strong: false,
        };
    }
    CompactCell {
        text: full,
        title: None,
        strong: false,
    }
}

fn strong(text: impl Into<String>) -> CompactCell {
    CompactCell {
        strong: true,
        ..cell(text)
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn named<'a>(run: &'a Run, module: &str, name: &str) -> impl Iterator<Item = &'a Block> {
    let (module, name) = (module.to_string(), name.to_string());
    run.blocks
        .iter()
        .filter(move |b| b.id.module() == module && b.id.name() == name)
}

fn field<'a>(fields: &'a Fields, key: &str) -> Option<&'a Value> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn field_text(fields: &Fields, key: &str) -> Option<String> {
    field(fields, key).map(format_value)
}

fn block_fields(block: &Block) -> Option<&Fields> {
    match &block.shape {
        Shape::Fields(fields) => Some(fields),
        _ => None,
    }
}

/// The rows of a table block as (column, value) lookups.
fn table_rows(block: &Block) -> Vec<Vec<(&str, &Value)>> {
    match &block.shape {
        Shape::Table { columns, rows, .. } => rows
            .iter()
            .map(|row| columns.iter().map(String::as_str).zip(row).collect())
            .collect(),
        _ => Vec::new(),
    }
}

fn col(row: &[(&str, &Value)], name: &str) -> Option<String> {
    row.iter()
        .find(|(c, _)| *c == name)
        .map(|(_, v)| format_value(v))
}

/// The compact view of `protocol` over `run`; `None` when none of its source blocks exists.
pub fn compact(run: &Run, protocol: Protocol) -> Option<CompactView> {
    match protocol {
        Protocol::Tls => tls(run),
        Protocol::Dns => dns(run),
        Protocol::Http => http(run),
        Protocol::Email => email(run),
        Protocol::Ip => ip(run),
    }
}

/// The port of a block qualifier `<address>:<port>`.
fn qualifier_port(block: &Block) -> Option<&str> {
    let q = block.id.qualifier()?;
    let tail = match q.strip_prefix('[') {
        Some(rest) => rest.split_once("]:")?.1,
        None => q.split_once(':')?.1,
    };
    Some(tail)
}

/// The address of a block qualifier, without brackets and port.
fn qualifier_address(block: &Block) -> String {
    let q = block.id.qualifier().unwrap_or_default();
    match q.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(rest).to_string(),
        None => q.split(':').next().unwrap_or(q).to_string(),
    }
}

/// The `CN=` value of a distinguished name, else the whole name.
fn common_name(dn: &str) -> String {
    dn.split(", ")
        .find_map(|part| part.strip_prefix("CN="))
        .unwrap_or(dn)
        .to_string()
}

/// The key as the chain shows it: a bit size after a named curve (P-256, Ed25519) is dropped.
fn key_text(key: &str) -> String {
    match key.rsplit_once(' ') {
        Some((kind, bits))
            if (kind.contains("P-") || kind.starts_with("Ed"))
                && !bits.is_empty()
                && bits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            kind.to_string()
        }
        _ => key.to_string(),
    }
}

fn fingerprints(block: &Block) -> Vec<Option<String>> {
    match &block.shape {
        Shape::Chain(items) => items
            .iter()
            .map(|item| field_text(item, "fingerprint_sha256"))
            .collect(),
        _ => Vec::new(),
    }
}

fn tls(run: &Run) -> Option<CompactView> {
    let chains: Vec<&Block> = named(run, "tls", "chain")
        .filter(|b| matches!(b.shape, Shape::Chain(_)))
        .collect();
    let Some(first) = chains.first() else {
        return tls_addresses(run);
    };
    let port = qualifier_port(first);
    let band: Vec<&Block> = chains
        .iter()
        .copied()
        .filter(|b| qualifier_port(b) == port)
        .collect();
    let consistent = band.iter().all(|b| fingerprints(b) == fingerprints(first));
    let mut caption = String::from("Certificate chain");
    if let Some(port) = port {
        caption.push_str(&format!("{SEP}port {port}"));
    }
    caption.push_str(&format!(
        "{SEP}{}",
        plural(band.len(), "address", "addresses")
    ));
    if consistent {
        caption.push_str(", consistent");
    } else {
        caption.push_str(&format!(
            ", chains differ{SEP}showing {}",
            qualifier_address(first)
        ));
    }
    let items = match &first.shape {
        Shape::Chain(items) => items.as_slice(),
        _ => &[],
    };
    let rows = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let leaf = i == 0;
            let subject = field_text(item, "subject").map(|s| common_name(&s));
            let issuer = if matches!(field(item, "self_signed"), Some(Value::Bool(true))) {
                "self-signed".to_string()
            } else {
                field_text(item, "issuer")
                    .map(|s| common_name(&s))
                    .unwrap_or_default()
            };
            let not_after = field_text(item, "not_after")
                .map(|t| t.split('T').next().unwrap_or(&t).to_string())
                .unwrap_or_default();
            let pick = |text: String| if leaf { strong(text) } else { cell(text) };
            vec![
                cell(i.to_string()),
                pick(subject.unwrap_or_default()),
                cell(issuer),
                cell(
                    field_text(item, "key")
                        .map(|k| key_text(&k))
                        .unwrap_or_default(),
                ),
                pick(not_after),
            ]
        })
        .collect();
    Some(CompactView {
        caption,
        columns: ["#", "Subject", "Issuer", "Key", "Not after"]
            .map(String::from)
            .to_vec(),
        rows,
        sources: band.iter().map(|b| b.id.clone()).collect(),
    })
}

fn tls_addresses(run: &Run) -> Option<CompactView> {
    let block = named(run, "tls", "addresses").next()?;
    let rows: Vec<Vec<CompactCell>> = table_rows(block)
        .iter()
        .map(|row| {
            ["address", "port", "outcome", "message"]
                .iter()
                .map(|name| cell(col(row, name).unwrap_or_default()))
                .collect()
        })
        .collect();
    Some(CompactView {
        caption: format!("Addresses{SEP}{} tested", rows.len()),
        columns: ["Address", "Port", "Outcome", "Message"]
            .map(String::from)
            .to_vec(),
        rows,
        sources: vec![block.id.clone()],
    })
}

fn dns(run: &Run) -> Option<CompactView> {
    let mut rows = Vec::new();
    let mut sources = Vec::new();
    for name in ["a", "aaaa", "ns", "caa", "ds"] {
        let Some(block) = named(run, "dns", name).next() else {
            continue;
        };
        sources.push(block.id.clone());
        match &block.shape {
            Shape::Records(records) if !records.is_empty() => {
                for r in records {
                    rows.push(vec![
                        cell(format_value(&r.rtype)),
                        cell(format_value(&r.data)),
                        cell(format_value(&r.ttl)),
                    ]);
                }
            }
            _ => rows.push(vec![
                cell(name.to_uppercase()),
                cell("no records"),
                cell(""),
            ]),
        }
    }
    if sources.is_empty() {
        return None;
    }
    let answered = run
        .blocks
        .iter()
        .filter(|b| b.id.module() == "dns")
        .filter(|b| matches!(&b.shape, Shape::Records(r) if !r.is_empty()))
        .count();
    Some(CompactView {
        caption: format!("Key records{SEP}{answered} record types answered"),
        columns: ["Type", "Data", "TTL"].map(String::from).to_vec(),
        rows,
        sources,
    })
}

const HTTP_HEADERS: [&str; 6] = [
    "strict-transport-security",
    "content-security-policy",
    "x-frame-options",
    "x-content-type-options",
    "referrer-policy",
    "permissions-policy",
];

fn http(run: &Run) -> Option<CompactView> {
    let server = named(run, "http", "server").next();
    let headers = named(run, "http", "headers").next();
    if server.is_none() && headers.is_none() {
        return None;
    }
    let mut caption = String::from("Response");
    if let Some(fields) = server.and_then(block_fields) {
        for key in ["status_code", "http_version", "final_url"] {
            if let Some(v) = field_text(fields, key) {
                caption.push_str(&format!("{SEP}{v}"));
            }
        }
    }
    let rows = headers
        .map(|block| {
            let table = table_rows(block);
            HTTP_HEADERS
                .iter()
                .map(|name| {
                    let value = table
                        .iter()
                        .find(|row| col(row, "name").as_deref() == Some(name))
                        .and_then(|row| col(row, "value"))
                        .unwrap_or_else(|| "not sent".to_string());
                    vec![cell(*name), cell(value)]
                })
                .collect()
        })
        .unwrap_or_default();
    Some(CompactView {
        caption,
        columns: ["Header", "Value"].map(String::from).to_vec(),
        rows,
        sources: [server, headers]
            .into_iter()
            .flatten()
            .map(|b| b.id.clone())
            .collect(),
    })
}

fn email(run: &Run) -> Option<CompactView> {
    let mut rows = Vec::new();
    let mut sources = Vec::new();
    let mut mx_hosts = None;
    for (name, label) in [
        ("mx", "MX"),
        ("spf", "SPF"),
        ("dmarc", "DMARC"),
        ("mta_sts", "MTA-STS"),
        ("tls_rpt", "TLS-RPT"),
        ("dkim", "DKIM"),
        ("bimi", "BIMI"),
    ] {
        let Some(block) = named(run, "email", name).next() else {
            continue;
        };
        sources.push(block.id.clone());
        match name {
            "mx" => {
                let hosts: Vec<String> = table_rows(block)
                    .iter()
                    .map(|row| {
                        let pref = col(row, "preference").unwrap_or_default();
                        let host = col(row, "host").unwrap_or_default();
                        format!("{pref} {host}").trim().to_string()
                    })
                    .collect();
                mx_hosts = Some(hosts.len());
                rows.push(vec![cell(label), cell(hosts.join(", "))]);
            }
            "dkim" => {
                for row in table_rows(block) {
                    let mut text = format!(
                        "{}: {}",
                        col(&row, "selector").unwrap_or_default(),
                        col(&row, "state").unwrap_or_default()
                    );
                    let known = |name: &str| col(&row, name).filter(|v| !v.is_empty());
                    if let Some(kind) = known("key_type") {
                        text.push_str(&format!(", {kind}"));
                        if let Some(bits) = known("bits") {
                            text.push_str(&format!(" {bits}"));
                        }
                    }
                    rows.push(vec![cell(label), cell(text)]);
                }
            }
            _ => {
                // A repeated record (two SPF records) is the fact that fails the check: one row each.
                for (key, value) in block_fields(block).into_iter().flatten() {
                    if key == "record" {
                        rows.push(vec![cell(label), cell(format_value(value))]);
                    }
                }
            }
        }
    }
    if sources.is_empty() {
        return None;
    }
    Some(CompactView {
        // Without an `email.mx` block the MX lookup was not answered: no count is claimed.
        caption: match mx_hosts {
            Some(n) => format!("Mail records{SEP}{}", plural(n, "MX host", "MX hosts")),
            None => "Mail records".to_string(),
        },
        columns: ["Record", "Value"].map(String::from).to_vec(),
        rows,
        sources,
    })
}

fn ip(run: &Run) -> Option<CompactView> {
    let block = named(run, "ip", "addresses").next()?;
    let rows: Vec<Vec<CompactCell>> = table_rows(block)
        .iter()
        .map(|row| {
            let listed: Vec<&str> = ["spamhaus", "c2", "tor", "vpn"]
                .into_iter()
                .filter(|flag| {
                    row.iter()
                        .any(|(c, v)| c == flag && matches!(v, Value::Bool(true)))
                })
                .collect();
            let listed = if listed.is_empty() {
                "none".to_string()
            } else {
                listed.join(", ")
            };
            let mut cells: Vec<CompactCell> = ["address", "asn", "org", "country", "network_type"]
                .iter()
                .map(|name| cell(col(row, name).unwrap_or_default()))
                .collect();
            cells.push(cell(listed));
            cells
        })
        .collect();
    Some(CompactView {
        caption: format!(
            "Addresses{SEP}{}",
            plural(rows.len(), "address", "addresses")
        ),
        columns: ["Address", "ASN", "Org", "Country", "Network", "Listed"]
            .map(String::from)
            .to_vec(),
        rows,
        sources: vec![block.id.clone()],
    })
}
