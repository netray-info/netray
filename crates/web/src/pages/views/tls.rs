//! The TLS port view (spec v2-tls-ui, requirements 1-9): a port's addresses, handshakes, chains
//! and SCTs render in one fragment at its `tls.addresses` block; every other block it takes
//! answers with an empty fragment. Which blocks are taken is one pure function of the siblings.

use std::net::IpAddr;

use askama::Template;
use netray_model::Protocol;
use netray_model::evidence::{Block, BlockId, Fields, Shape, Value};
use netray_tls::module::{REGISTER, band_port};
use netray_tls::validate::chain_trust::TRUST_STORE;

use super::validity::{self, ValidityState};
use super::{ViewContext, frame, tone};
use crate::components::{Cell, FactCell, FactsGridView, Tone, facts_grid_with, line_tone};
use crate::evidence::{cell, display_value, unix_time};
use crate::spectrum::mini_band;

const ADDRESS_COLUMNS: [&str; 4] = ["address", "port", "outcome", "message"];
const CT_COLUMNS: [&str; 3] = ["address", "sct_count", "log_ids"];
const MINI_WIDTH: u32 = 96;
const MINI_HEIGHT: f64 = 22.0;

pub(super) fn render(block: &Block, ctx: &ViewContext<'_>, siblings: &[Block]) -> Option<String> {
    if block.id.module() != "tls" {
        return None;
    }
    let siblings = if siblings.is_empty() {
        std::slice::from_ref(block)
    } else {
        siblings
    };
    let ports = taken(siblings);
    if let Some(port) = ports.iter().find(|p| p.head.id == block.id) {
        return port_view(port, ctx);
    }
    ports.iter().any(|p| p.takes(&block.id)).then(String::new)
}

/// One row of the addresses table.
struct Row<'a> {
    addr: IpAddr,
    value: &'a Value,
    outcome: &'a str,
    message: &'a str,
}

/// A taken chain: its address, its block and its certificates in served order.
type Chain<'a> = (IpAddr, &'a Block, &'a [Fields]);

/// A port's blocks the view takes.
struct Port<'a> {
    head: &'a Block,
    port: i64,
    rows: Vec<Row<'a>>,
    chains: Vec<Chain<'a>>,
    handshakes: Vec<(IpAddr, &'a Block, &'a Fields)>,
    ct: Option<(&'a Block, &'a [Vec<Value>])>,
    protocols: Option<(&'a Block, &'a Fields)>,
    probes: Vec<(IpAddr, &'a Block, &'a Fields)>,
    notices: Option<(&'a Block, &'a Fields)>,
}

impl Port<'_> {
    fn takes(&self, id: &BlockId) -> bool {
        self.chains.iter().any(|(_, b, _)| &b.id == id)
            || self.handshakes.iter().any(|(_, b, _)| &b.id == id)
            || self.ct.is_some_and(|(b, _)| &b.id == id)
            || self.protocols.is_some_and(|(b, _)| &b.id == id)
            || self.probes.iter().any(|(_, b, _)| &b.id == id)
            || self.notices.is_some_and(|(b, _)| &b.id == id)
    }

    fn chain(&self, addr: IpAddr) -> Option<&Chain<'_>> {
        self.chains.iter().find(|(a, _, _)| *a == addr)
    }

    fn handshake(&self, addr: IpAddr) -> Option<&Fields> {
        self.handshakes
            .iter()
            .find(|(a, _, _)| *a == addr)
            .map(|(_, _, f)| *f)
    }
}

/// The ports among `siblings` and the blocks each takes: the addresses table of the four
/// columns; each chain and handshake of an `ok` address whose keys are all registered (a
/// handshake only beside a taken chain, which shows its OCSP facts); the SCT table of the
/// three columns when every row's address has a taken chain; the protocols of the port and
/// each probe block of an `ok` address with registered keys; the notices on the band port.
fn taken(siblings: &[Block]) -> Vec<Port<'_>> {
    let registered = |name: &str| -> Vec<&str> {
        REGISTER
            .keys
            .iter()
            .filter(|(b, _, _)| b.strip_prefix("tls.") == Some(name))
            .map(|(_, k, _)| *k)
            .collect()
    };
    let chain_keys = registered("chain");
    let handshake_keys = registered("handshake");
    let protocol_keys = registered("protocols");
    let probe_keys = registered("probes");
    let notice_keys = registered("notices");
    let known =
        |fields: &Fields, keys: &[&str]| fields.iter().all(|(k, _)| keys.contains(&k.as_str()));
    let of = |name: &'static str| {
        siblings
            .iter()
            .filter(move |b| b.id.module() == "tls" && b.id.name() == name)
    };
    let mut ports = Vec::new();
    for head in of("addresses") {
        let Some((port, rows)) = address_rows(head) else {
            continue;
        };
        let ok = |id: &BlockId| {
            at(id).filter(|(addr, p)| {
                *p == port && rows.iter().any(|r| r.addr == *addr && r.outcome == "ok")
            })
        };
        let chains: Vec<_> = of("chain")
            .filter_map(|b| match &b.shape {
                Shape::Chain(certs)
                    if !certs.is_empty() && certs.iter().all(|c| known(c, &chain_keys)) =>
                {
                    ok(&b.id).map(|(addr, _)| (addr, b, certs.as_slice()))
                }
                _ => None,
            })
            .collect();
        let handshakes = of("handshake")
            .filter_map(|b| match &b.shape {
                Shape::Fields(fields) if known(fields, &handshake_keys) => ok(&b.id)
                    .filter(|(addr, _)| chains.iter().any(|(a, _, _)| a == addr))
                    .map(|(addr, _)| (addr, b, fields)),
                _ => None,
            })
            .collect();
        let ct = of("ct")
            .filter(|b| b.id.qualifier() == head.id.qualifier())
            .find_map(|b| match &b.shape {
                Shape::Table { key, columns, rows }
                    if key == "address"
                        && columns.iter().map(String::as_str).eq(CT_COLUMNS)
                        && rows.iter().all(|r| {
                            r.len() == CT_COLUMNS.len()
                                && matches!(&r[0], Value::Addr(a)
                                    if chains.iter().any(|(c, _, _)| c == a))
                        }) =>
                {
                    Some((b, rows.as_slice()))
                }
                _ => None,
            });
        let protocols = of("protocols")
            .filter(|b| b.id.qualifier() == head.id.qualifier())
            .find_map(|b| match &b.shape {
                Shape::Fields(fields) if known(fields, &protocol_keys) => Some((b, fields)),
                _ => None,
            });
        let probes = of("probes")
            .filter_map(|b| match &b.shape {
                Shape::Fields(fields) if known(fields, &probe_keys) => {
                    ok(&b.id).map(|(addr, _)| (addr, b, fields))
                }
                _ => None,
            })
            .collect();
        ports.push(Port {
            head,
            port,
            rows,
            chains,
            handshakes,
            ct,
            protocols,
            probes,
            notices: None,
        });
    }
    let band = band_port(ports.iter().filter_map(|p| u16::try_from(p.port).ok()));
    let notices = of("notices").find_map(|b| match &b.shape {
        Shape::Fields(fields) if known(fields, &notice_keys) => Some((b, fields)),
        _ => None,
    });
    if let Some(port) = ports.iter_mut().find(|p| p.port == i64::from(band)) {
        port.notices = notices;
    }
    ports
}

/// The port and rows of an addresses table holding exactly the four columns, one port.
fn address_rows(head: &Block) -> Option<(i64, Vec<Row<'_>>)> {
    let Shape::Table { key, columns, rows } = &head.shape else {
        return None;
    };
    if key != "address" || !columns.iter().map(String::as_str).eq(ADDRESS_COLUMNS) {
        return None;
    }
    let rows: Vec<(i64, Row)> = rows
        .iter()
        .map(|r| match r.as_slice() {
            [
                value @ Value::Addr(addr),
                Value::Int(port),
                Value::Text(outcome),
                Value::Text(message),
            ] => Some((
                *port,
                Row {
                    addr: *addr,
                    value,
                    outcome,
                    message,
                },
            )),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let port = rows.first()?.0;
    rows.iter()
        .all(|(p, _)| *p == port)
        .then(|| (port, rows.into_iter().map(|(_, r)| r).collect()))
}

/// The address and port of a qualifier `<addr>:<port>` or `[<v6>]:<port>`.
fn at(id: &BlockId) -> Option<(IpAddr, i64)> {
    let (host, port) = id.qualifier()?.rsplit_once(':')?;
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    Some((host.parse().ok()?, port.parse().ok()?))
}

/// Each distinct value once, with the addresses reporting it when the values differ or when
/// fewer than the `group`'s addresses hold one.
fn distinct<T: PartialEq>(values: Vec<(IpAddr, T)>, group: usize) -> Vec<(T, Option<String>)> {
    let partial = values.len() < group;
    let mut groups: Vec<(T, Vec<IpAddr>)> = Vec::new();
    for (addr, value) in values {
        match groups.iter_mut().find(|(v, _)| *v == value) {
            Some((_, addrs)) => addrs.push(addr),
            None => groups.push((value, vec![addr])),
        }
    }
    let named = partial || groups.len() > 1;
    groups
        .into_iter()
        .map(|(value, addrs)| {
            let list = addrs
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            (value, named.then_some(list))
        })
        .collect()
}

/// Distinct texts as "v" or "v (a, b)".
fn distinct_texts(values: Vec<(IpAddr, String)>, group: usize) -> Vec<String> {
    distinct(values, group)
        .into_iter()
        .map(|(v, addrs)| match addrs {
            Some(addrs) => format!("{v} ({addrs})"),
            None => v,
        })
        .collect()
}

fn get<'f>(fields: &'f Fields, key: &str) -> Option<&'f Value> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The date `YYYY-MM-DD` of a time value, else its display text.
fn day(value: &Value) -> String {
    let text = display_value(value);
    match value {
        Value::Time(_) => text.get(..10).map_or(text.clone(), str::to_string),
        _ => text,
    }
}

fn esc(text: &str) -> Option<String> {
    askama::filters::escape(text, askama::filters::Html)
        .ok()
        .map(|s| s.to_string())
}

fn chip(tone: Tone, text: &str) -> Option<String> {
    Some(format!(
        "<span class=\"tls-chip\" data-tone=\"{}\">{}</span>",
        tone.as_str(),
        esc(text)?
    ))
}

/// The common name of a distinguished name, else the name.
fn common_name(dn: &str) -> String {
    dn.split(',')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("CN="))
        .unwrap_or(dn)
        .to_string()
}

/// `4f:9a:…:e1`: the first two and the last byte of a fingerprint, lower case.
fn leaf_short(fingerprint: &str) -> String {
    let lower = fingerprint.to_lowercase();
    let bytes: Vec<&str> = lower.split(':').collect();
    match bytes.as_slice() {
        [a, b, .., z] if bytes.len() > 3 => format!("{a}:{b}:…:{z}"),
        _ => lower.clone(),
    }
}

struct Rect {
    x: String,
    y: String,
    width: String,
    height: String,
    tone: &'static str,
}

struct Leaf {
    short: String,
    full: String,
    marker: Option<(&'static str, &'static str)>,
}

struct AddressRow {
    address: Cell,
    error: Option<(String, String)>,
    version: String,
    cipher: String,
    leaf: Option<Leaf>,
}

#[derive(Template)]
#[template(path = "partials/tls_addresses.html")]
struct AddressesView {
    rows: Vec<AddressRow>,
    band: Option<Vec<Rect>>,
    band_width: u32,
    band_height: f64,
    note: String,
    anchors: Vec<String>,
}

struct ValidityView {
    state: &'static str,
    before: String,
    centre: String,
    after: String,
    marker_x: u32,
    warn_x: u32,
    warn_width: u32,
    caption: String,
}

struct Pill {
    name: String,
    href: Option<String>,
}

struct ChainCert {
    name: String,
    grid: String,
}

#[derive(Template)]
#[template(path = "partials/tls_certificate.html")]
struct CertificateView {
    validity: Option<ValidityView>,
    grid: String,
    pills: Vec<Pill>,
    inferred: Option<String>,
    trust: Option<(&'static str, String)>,
    more: Vec<ChainCert>,
    anchors: Vec<String>,
}

/// The whole port: the notices, the Addresses section, one Certificate section per distinct
/// chain, then the Protocols and features section.
fn port_view(port: &Port<'_>, ctx: &ViewContext<'_>) -> Option<String> {
    let mut html = match port.notices {
        Some(notices) => notices_banner(port, notices, ctx)?,
        None => String::new(),
    };
    html.push_str(&addresses_section(port, ctx)?);
    let mut groups: Vec<(Vec<String>, Vec<&Chain<'_>>)> = Vec::new();
    for row in &port.rows {
        let Some(chain) = port.chain(row.addr) else {
            continue;
        };
        let key: Vec<String> = chain
            .2
            .iter()
            .map(|c| match get(c, "fingerprint_sha256") {
                Some(v) => display_value(v),
                None => format!("{c:?}"),
            })
            .collect();
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, members)) => members.push(chain),
            None => groups.push((key, vec![chain])),
        }
    }
    let several = groups.len() > 1;
    let inner = ViewContext {
        statuses: ctx.statuses,
        decorate: ctx.decorate,
        feeds: &[],
    };
    for (_, members) in &groups {
        let title = if several {
            let addrs: Vec<String> = members.iter().map(|(a, _, _)| a.to_string()).collect();
            format!("Certificate · {}", addrs.join(", "))
        } else {
            "Certificate".to_string()
        };
        let body = certificate(port, members, several, ctx)?;
        html.push_str(&frame(members[0].1, &title, &body, &inner)?);
    }
    if port.protocols.is_some() || !port.handshakes.is_empty() || !port.probes.is_empty() {
        let body = protocols(port, ctx)?;
        let title = "Protocols and features";
        html.push_str(&match port.protocols {
            Some((block, _)) => frame(block, title, &body, &inner)?,
            None => format!(
                "<section class=\"protocol-view\"><h3 class=\"protocol-view__title\">{title}</h3>\n<div class=\"protocol-view__card\">{body}</div>\n</section>"
            ),
        });
    }
    Some(html)
}

#[derive(Template)]
#[template(path = "partials/tls_notices.html")]
struct NoticesView {
    line: Option<String>,
    skipped: Vec<Cell>,
    warnings: Vec<String>,
    anchor: String,
}

/// The warn banner of the port's notices: the skipped addresses, then each warning.
fn notices_banner(
    port: &Port<'_>,
    (block, fields): (&Block, &Fields),
    ctx: &ViewContext<'_>,
) -> Option<String> {
    let skipped: Vec<Cell> = fields
        .iter()
        .filter(|(k, _)| k == "skipped_address")
        .map(|(k, v)| cell(&block.id, k, v, ctx.decorate))
        .collect();
    let count = |n: usize| {
        if n == 1 {
            "1 address".to_string()
        } else {
            format!("{n} addresses")
        }
    };
    NoticesView {
        line: (!skipped.is_empty()).then(|| {
            format!(
                "{} not inspected: the inspector checks at most {} per host",
                count(skipped.len()),
                count(port.rows.len())
            )
        }),
        skipped,
        warnings: fields
            .iter()
            .filter(|(k, _)| k == "warning")
            .map(|(_, v)| display_value(v))
            .collect(),
        anchor: block.id.to_string(),
    }
    .render()
    .ok()
}

/// Which bar tones a cell's state allows: a bar never contradicts its cell.
#[derive(Clone, Copy)]
enum Fit {
    Any,
    Good,
    Bad,
}

impl Fit {
    fn of_version(word: &str) -> Fit {
        match word {
            "disabled" => Fit::Good,
            "enabled" => Fit::Bad,
            _ => Fit::Any,
        }
    }

    fn admits(self, tone: &str) -> bool {
        match self {
            Fit::Any => true,
            Fit::Good => tone == "pass",
            Fit::Bad => tone != "pass",
        }
    }
}

struct ProtocolCell {
    name: &'static str,
    state: String,
    tone: Option<&'static str>,
    href: Option<String>,
}

struct ProbeRow {
    name: String,
    cells: Vec<Option<(String, String)>>,
}

#[derive(Template)]
#[template(path = "partials/tls_protocols.html")]
struct ProtocolsView {
    cells: Vec<ProtocolCell>,
    addrs: Vec<IpAddr>,
    probes: Vec<ProbeRow>,
    anchors: Vec<String>,
}

/// The probe keys' row names, in register order.
const PROBE_NAMES: [(&str, &str); 8] = [
    ("ssl3", "SSL 3.0"),
    ("tls1_0", "TLS 1.0"),
    ("tls1_1", "TLS 1.1"),
    ("tls1_2", "TLS 1.2"),
    ("tls1_3", "TLS 1.3"),
    ("suites_insecure", "insecure suites"),
    ("suites_cbc", "CBC suites"),
    ("suites_static_rsa", "static RSA suites"),
];

/// The Protocols and features grid and the probe answers of the port.
fn protocols(port: &Port<'_>, ctx: &ViewContext<'_>) -> Option<String> {
    let proto = |key: &str| port.protocols.and_then(|(_, f)| get(f, key));
    let judged = |id: &str| tone(ctx, id).map(|t| t.as_str());
    let per = |key: &str, show: &dyn Fn(&Value) -> String| {
        distinct_texts(
            port.handshakes
                .iter()
                .filter_map(|(addr, _, f)| get(f, key).map(|v| (*addr, show(v))))
                .collect(),
            port.handshakes.len(),
        )
    };
    let mut cells = Vec::new();
    let mut push = |name: &'static str, state: String, check: Option<&str>, fit: Fit| {
        cells.push(ProtocolCell {
            name,
            state,
            tone: check.and_then(judged).filter(|t| fit.admits(t)),
            href: None,
        });
    };
    for (name, key, check) in [
        ("TLS 1.3", "tls1_3", "tls.tls_version"),
        ("TLS 1.2", "tls1_2", "tls.tls_version"),
        ("TLS 1.1", "tls1_1", "tls.legacy_versions"),
        ("TLS 1.0", "tls1_0", "tls.legacy_versions"),
    ] {
        if let Some(v) = proto(key) {
            let legacy = matches!(key, "tls1_1" | "tls1_0");
            let word = display_value(v);
            let fit = if legacy {
                Fit::of_version(&word)
            } else {
                Fit::Any
            };
            let state = if legacy && word == "disabled" {
                "disabled, as it should be".to_string()
            } else {
                word
            };
            push(name, state, Some(check), fit);
        }
    }
    let alpn = per("alpn", &display_value);
    let alpn = if alpn.is_empty() {
        "none".to_string()
    } else {
        alpn.join(" · ")
    };
    push("ALPN", alpn, Some("tls.alpn_consistency"), Fit::Any);
    if let Some(v) = proto("forward_secrecy_all_suites") {
        let (state, fit) = match v {
            Value::Bool(true) => ("all suites".to_string(), Fit::Good),
            Value::Bool(false) => ("not all suites".to_string(), Fit::Bad),
            v => (display_value(v), Fit::Any),
        };
        push("Forward secrecy", state, Some("tls.weak_suites"), fit);
    }
    let ech = per("ech", &|v| match v {
        Value::Bool(true) => "advertised".to_string(),
        Value::Bool(false) => "not advertised".to_string(),
        v => display_value(v),
    });
    if !ech.is_empty() {
        push(
            "Encrypted Client Hello",
            ech.join(" · "),
            Some("tls.ech_advertised"),
            Fit::Any,
        );
    }
    let host = port
        .handshakes
        .iter()
        .find_map(|(_, _, f)| get(f, "sni").map(display_value));
    if let Some(host) = host {
        cells.push(ProtocolCell {
            name: "HSTS",
            state: "checked in the HTTP inspector".to_string(),
            tone: None,
            href: Some(format!("/inspect/http?url=https://{host}/")),
        });
    }
    let mut push = |name: &'static str, state: String, check: Option<&str>, fit: Fit| {
        cells.push(ProtocolCell {
            name,
            state,
            tone: check.and_then(judged).filter(|t| fit.admits(t)),
            href: None,
        });
    };
    if let Some(v) = proto("ssl3") {
        let word = display_value(v);
        let fit = Fit::of_version(&word);
        push("SSL 3.0", word, Some("tls.legacy_versions"), fit);
    }
    let groups: Vec<(&str, String)> = [
        ("insecure", "suites_insecure"),
        ("CBC", "suites_cbc"),
        ("static RSA", "suites_static_rsa"),
    ]
    .into_iter()
    .filter_map(|(label, key)| proto(key).map(|v| (label, display_value(v))))
    .collect();
    if !groups.is_empty() {
        let fit = if groups.iter().any(|(_, w)| w == "accepted") {
            Fit::Bad
        } else if groups.iter().all(|(_, w)| w == "refused") {
            Fit::Good
        } else {
            Fit::Any
        };
        let suites: Vec<String> = groups.iter().map(|(l, w)| format!("{l} {w}")).collect();
        push(
            "Cipher suites",
            suites.join(" · "),
            Some("tls.weak_suites"),
            fit,
        );
    }
    let kx = per("key_exchange_group", &display_value);
    if !kx.is_empty() {
        push("Key exchange", kx.join(" · "), None, Fit::Any);
    }
    let sni = per("sni", &display_value);
    if !sni.is_empty() {
        push("SNI", sni.join(" · "), None, Fit::Any);
    }
    let starttls = per("starttls", &display_value);
    if !starttls.is_empty() {
        push("STARTTLS", starttls.join(" · "), None, Fit::Any);
    }

    let addrs: Vec<IpAddr> = port
        .rows
        .iter()
        .filter(|r| port.probes.iter().any(|(a, _, _)| *a == r.addr))
        .map(|r| r.addr)
        .collect();
    let probe = |addr: IpAddr| {
        port.probes
            .iter()
            .find(|(a, _, _)| *a == addr)
            .map(|(_, _, f)| *f)
    };
    let probes = PROBE_NAMES
        .iter()
        .filter_map(|(key, name)| {
            let answer = format!("{key}_answer");
            let cells: Vec<Option<(String, String)>> = addrs
                .iter()
                .map(|a| {
                    let f = probe(*a)?;
                    let word = get(f, key).map(display_value);
                    let said = get(f, &answer).map(display_value);
                    (word.is_some() || said.is_some())
                        .then(|| (word.unwrap_or_default(), said.unwrap_or_default()))
                })
                .collect();
            cells.iter().any(Option::is_some).then(|| ProbeRow {
                name: name.to_string(),
                cells,
            })
        })
        .collect();
    ProtocolsView {
        cells,
        addrs,
        probes,
        anchors: port
            .probes
            .iter()
            .map(|(_, b, _)| b.id.to_string())
            .collect(),
    }
    .render()
    .ok()
}

/// The part of a TLS run holding the band port's blocks: the only part, or the `port <n>`
/// part of [`band_port`] over the parts' ports.
pub fn tls_band_part(labels: &[String]) -> Option<usize> {
    if labels.len() == 1 {
        return Some(0);
    }
    let ports: Vec<(usize, u16)> = labels
        .iter()
        .enumerate()
        .filter_map(|(n, l)| Some((n, l.strip_prefix("port ")?.parse().ok()?)))
        .collect();
    let band = band_port(ports.iter().map(|(_, p)| *p));
    ports.iter().find(|(_, p)| *p == band).map(|(n, _)| *n)
}

/// The related-records summary of a TLS port's blocks: its CAA and TLSA records and whether
/// a handshake saw ECH advertised in the HTTPS record.
pub fn tls_related_summary(blocks: &[Block]) -> String {
    let of = |name: &str| {
        blocks
            .iter()
            .find(|b| b.id.module() == "tls" && b.id.name() == name)
    };
    let records = |name: &str| match of(name).map(|b| &b.shape) {
        Some(Shape::Records(rows)) => Some(rows.len()),
        _ => None,
    };
    let caa = match records("caa") {
        Some(0) => "CAA (none published)".to_string(),
        Some(1) => "CAA (1 record)".to_string(),
        Some(n) => format!("CAA ({n} records)"),
        None => "CAA (not measured)".to_string(),
    };
    let tlsa = match records("tlsa") {
        Some(0) => "TLSA (none)".to_string(),
        Some(n) => format!("TLSA ({n})"),
        None => "TLSA (not measured)".to_string(),
    };
    let ech = blocks
        .iter()
        .filter(|b| b.id.module() == "tls" && b.id.name() == "handshake")
        .find_map(|b| match &b.shape {
            Shape::Fields(fields) => get(fields, "ech"),
            _ => None,
        });
    let https = match ech {
        Some(Value::Bool(true)) => "HTTPS (ech=)",
        Some(Value::Bool(false)) => "HTTPS (no ech=)",
        _ => "HTTPS (not measured)",
    };
    format!("{caa} · {tlsa} · {https}")
}

fn addresses_section(port: &Port<'_>, ctx: &ViewContext<'_>) -> Option<String> {
    let band = mini_band(ctx.statuses, Protocol::Tls, MINI_WIDTH).map(|bars| {
        bars.iter()
            .map(|bar| {
                let height = bar.height * MINI_HEIGHT;
                Rect {
                    x: format!("{:.2}", bar.x),
                    y: format!("{:.2}", MINI_HEIGHT - height),
                    width: format!("{:.2}", bar.width),
                    height: format!("{height:.2}"),
                    tone: line_tone(bar.tone).as_str(),
                }
            })
            .collect()
    });
    let fingerprint = |addr: IpAddr| {
        port.chain(addr)
            .and_then(|(_, _, certs)| get(&certs[0], "fingerprint_sha256"))
            .map(display_value)
    };
    let first = port.rows.iter().find_map(|r| fingerprint(r.addr));
    let leaves = port
        .rows
        .iter()
        .filter(|r| fingerprint(r.addr).is_some())
        .count();
    let rows = port
        .rows
        .iter()
        .map(|row| {
            let hs = port.handshake(row.addr);
            let text = |key: &str| {
                hs.and_then(|f| get(f, key))
                    .map(display_value)
                    .unwrap_or_default()
            };
            let leaf = fingerprint(row.addr).map(|full| Leaf {
                short: leaf_short(&full),
                marker: (leaves > 1).then(|| {
                    if Some(&full) == first.as_ref() {
                        ("pass", "same")
                    } else {
                        ("warn", "differs")
                    }
                }),
                full,
            });
            AddressRow {
                address: cell(&port.head.id, "address", row.value, ctx.decorate),
                error: (row.outcome != "ok")
                    .then(|| (row.outcome.to_string(), row.message.to_string())),
                version: text("version"),
                cipher: text("cipher_suite"),
                leaf,
            }
        })
        .collect();
    let per = |key: &str, show: &dyn Fn(&Value) -> String| {
        distinct_texts(
            port.handshakes
                .iter()
                .filter_map(|(addr, _, f)| get(f, key).map(|v| (*addr, show(v))))
                .collect(),
            port.handshakes.len(),
        )
    };
    let mut note = format!("Port {}", port.port);
    let durations = per("handshake_ms", &display_value);
    if !durations.is_empty() {
        note.push_str(&format!(" · handshake {}", durations.join(" · ")));
    }
    let mut anchors: Vec<String> = port
        .handshakes
        .iter()
        .map(|(_, b, _)| b.id.to_string())
        .collect();
    anchors.extend(port.ct.map(|(b, _)| b.id.to_string()));
    let body = AddressesView {
        rows,
        band,
        band_width: MINI_WIDTH,
        band_height: MINI_HEIGHT,
        note,
        anchors,
    }
    .render()
    .ok()?;
    frame(port.head, "Addresses", &body, ctx)
}

/// A labelled cell of every value of `key` in `cert`.
fn fact(
    block: &Block,
    cert: &Fields,
    label: &str,
    key: &str,
    ctx: &ViewContext<'_>,
) -> Option<FactCell> {
    let values: Vec<Cell> = cert
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| {
            let mut c = cell(&block.id, key, v, ctx.decorate);
            if key == "sans" {
                c.href = san_href(&c.text);
            }
            c
        })
        .collect();
    (!values.is_empty()).then(|| FactCell {
        label: label.to_string(),
        values,
        chips: false,
        tone: None,
    })
}

/// The inspector a SAN links to: an address to the IP inspector, a DNS name to the DNS
/// inspector; a wildcard to none.
fn san_href(san: &str) -> Option<String> {
    if san.starts_with('*') {
        return None;
    }
    match san.parse::<IpAddr>() {
        Ok(addr) => Some(crate::links::ip_href(addr)),
        Err(_) => Some(crate::links::dns_query_href(san)),
    }
}

/// "1 day" or "<n> days".
fn days(n: i64) -> String {
    if n == 1 {
        "1 day".to_string()
    } else {
        format!("{n} days")
    }
}

fn texts(label: &str, values: Vec<String>) -> Option<FactCell> {
    (!values.is_empty()).then(|| FactCell {
        label: label.to_string(),
        values: values.into_iter().map(Cell::from).collect(),
        chips: false,
        tone: None,
    })
}

/// The fail chips of a certificate's true `self_signed` and `expired`.
fn flags(cert: &Fields) -> Option<String> {
    let mut out = String::new();
    for (key, text) in [("self_signed", "self-signed"), ("expired", "expired")] {
        if matches!(get(cert, key), Some(Value::Bool(true))) {
            out.push(' ');
            out.push_str(&chip(Tone::Fail, text)?);
        }
    }
    Some(out)
}

/// The validity facts as cells, for a certificate the bar does not show.
fn validity_cells(cert: &Fields) -> Vec<FactCell> {
    [
        ("Not before", "not_before"),
        ("Not after", "not_after"),
        ("Days left", "days_remaining"),
    ]
    .into_iter()
    .filter_map(|(label, key)| {
        let text = match get(cert, key)? {
            Value::Int(n) => format!("{n} days"),
            v => day(v),
        };
        texts(label, vec![text])
    })
    .collect()
}

/// A facts grid of `cells` with their trails.
fn grid(cells: Vec<(Option<FactCell>, String)>) -> Option<String> {
    let (cells, trails): (Vec<FactCell>, Vec<String>) = cells
        .into_iter()
        .filter_map(|(c, t)| c.map(|c| (c, t)))
        .unzip();
    facts_grid_with(&FactsGridView { cells }, &trails).ok()
}

/// One Certificate section's body for the addresses in `members`, which serve one chain.
/// The trust text shows only when the port serves one chain (`several` false).
fn certificate(
    port: &Port<'_>,
    members: &[&Chain<'_>],
    several: bool,
    ctx: &ViewContext<'_>,
) -> Option<String> {
    let (_, block, certs) = members[0];
    let leaf = &certs[0];
    let measured = match (get(leaf, "not_after"), get(leaf, "days_remaining")) {
        (Some(Value::Time(na)), Some(Value::Int(left))) => {
            unix_time(na).map(|end| end - left * 86_400)
        }
        _ => None,
    };
    let now = measured.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    });
    let layout = match (get(leaf, "not_before"), get(leaf, "not_after")) {
        (Some(Value::Time(nb)), Some(Value::Time(na))) => validity::layout(nb, na, now).map(|l| {
            (
                l,
                day(&Value::Time(nb.clone())),
                day(&Value::Time(na.clone())),
            )
        }),
        _ => None,
    };
    let validity = layout.as_ref().map(|(l, before, after)| ValidityView {
        state: l.state.as_str(),
        before: format!("not before {before}"),
        centre: match l.state {
            ValidityState::Valid => format!("today · {} left", days(l.days_left)),
            ValidityState::Expired => match -l.days_left {
                0 => "expired today".to_string(),
                n => format!("expired {} ago", days(n)),
            },
            ValidityState::NotYetValid => match -l.elapsed_days {
                0 => "not valid for less than a day".to_string(),
                n => format!("not valid for {}", days(n)),
            },
        },
        after: format!("not after {after}"),
        marker_x: l.marker_x,
        warn_x: l.warn_x,
        warn_width: validity::BAR_WIDTH - l.warn_x,
        caption: format!("the last {} days turn the expiry line amber", l.warn_days),
    });

    let addrs: Vec<IpAddr> = members.iter().map(|(a, _, _)| *a).collect();
    let mut ocsp = Vec::new();
    let mut fresh = Vec::new();
    let mut live = Vec::new();
    for addr in &addrs {
        let Some(hs) = port.handshake(*addr) else {
            continue;
        };
        let mut parts: Vec<String> = Vec::new();
        match get(hs, "ocsp_stapled") {
            Some(Value::Bool(true)) => parts.push("stapled".into()),
            Some(Value::Bool(false)) => parts.push("not stapled".into()),
            Some(v) => parts.push(display_value(v)),
            None => {}
        }
        parts.extend(get(hs, "ocsp_status").map(display_value));
        parts.extend(get(hs, "ocsp_this_update").map(day));
        if !parts.is_empty() {
            ocsp.push((*addr, parts.join(" · ")));
        }
        if let Some(v) = get(hs, "ocsp_next_update") {
            fresh.push((*addr, format!("fresh until {}", day(v))));
        }
        if let Some(status) = get(hs, "ocsp_live_status") {
            let status = display_value(status);
            let mut text = format!("live: {status}");
            if let Some(at) = get(hs, "ocsp_live_revoked_at") {
                text.push_str(&format!(" {}", day(at)));
            }
            if let Some(reason) = get(hs, "ocsp_live_reason") {
                text.push_str(&format!(" · {}", display_value(reason)));
            }
            let tone = match status.as_str() {
                "good" => Tone::Pass,
                "revoked" => Tone::Fail,
                _ => Tone::Warn,
            };
            live.push((*addr, (text, tone)));
        }
    }
    let mut ocsp_values = distinct_texts(ocsp, addrs.len());
    ocsp_values.extend(distinct_texts(fresh, addrs.len()));
    let mut live_trail = String::new();
    for ((text, tone), addrs) in distinct(live, addrs.len()) {
        live_trail.push(' ');
        live_trail.push_str(&chip(tone, &text)?);
        if let Some(addrs) = addrs {
            live_trail.push_str(&format!(" ({})", esc(&addrs)?));
        }
    }
    let ocsp_cell = if ocsp_values.is_empty() && !live_trail.is_empty() {
        texts("OCSP", vec![String::new()])
    } else {
        texts("OCSP", ocsp_values)
    };

    let ct_rows: Vec<&Vec<Value>> = port
        .ct
        .map(|(_, rows)| {
            rows.iter()
                .filter(|r| matches!(&r[0], Value::Addr(a) if addrs.contains(a)))
                .collect()
        })
        .unwrap_or_default();
    let ct_of = |show: &dyn Fn(&Value) -> String, at: usize| {
        distinct_texts(
            ct_rows
                .iter()
                .filter_map(|r| match &r[0] {
                    Value::Addr(a) => Some((*a, show(&r[at]))),
                    _ => None,
                })
                .collect(),
            addrs.len(),
        )
    };
    let scts = ct_of(
        &|v| match v {
            Value::Int(1) => "1 SCT".to_string(),
            Value::Int(n) => format!("{n} SCTs"),
            v => display_value(v),
        },
        1,
    );
    let logs = ct_of(&display_value, 2);

    let f = |label: &str, key: &str| fact(block, leaf, label, key, ctx);
    let mut cells = vec![
        (f("Subject", "subject"), flags(leaf)?),
        (f("Alternative names", "sans"), String::new()),
        (f("Issuer", "issuer"), String::new()),
        (f("Key", "key"), String::new()),
        (f("Signature", "signature_algorithm"), String::new()),
        (ocsp_cell, live_trail),
        (texts("Certificate Transparency", scts), String::new()),
        (f("Serial", "serial"), String::new()),
        (f("SHA-256", "fingerprint_sha256"), String::new()),
        (f("SHA-1", "fingerprint_sha1"), String::new()),
        (f("Policy", "policy"), String::new()),
        (f("OCSP responder", "ocsp_url"), String::new()),
        (f("CA issuers", "ca_issuers_url"), String::new()),
        (texts("SCT logs", logs), String::new()),
    ];
    if layout.is_none() {
        cells.extend(
            validity_cells(leaf)
                .into_iter()
                .map(|c| (Some(c), String::new())),
        );
    }
    let grid_html = grid(cells)?;

    let pills = certs
        .iter()
        .map(|c| Pill {
            name: get(c, "subject").map_or_else(String::new, |v| common_name(&display_value(v))),
            href: get(c, "fingerprint_sha256")
                .map(|v| format!("https://crt.sh/?q={}", display_value(v).replace(':', ""))),
        })
        .collect();
    let last = certs.last()?;
    let trusted = tone(ctx, "tls.chain_trusted");
    let inferred = match (get(last, "self_signed"), get(last, "issuer")) {
        (Some(Value::Bool(false)), Some(issuer)) if trusted == Some(Tone::Pass) => {
            Some(common_name(&display_value(issuer))).filter(|n| !n.is_empty())
        }
        _ => None,
    };
    let trust = match trusted.filter(|_| !several) {
        Some(Tone::Pass) => Some(("pass", format!("trusted by {TRUST_STORE}"))),
        Some(t @ (Tone::Warn | Tone::Fail)) => netray_engine::catalogue::catalogue()
            .entries()
            .iter()
            .find(|s| s.id.to_string() == "tls.chain_trusted")
            .map(|s| (t.as_str(), s.finding.clone())),
        _ => None,
    };
    let more = certs[1..]
        .iter()
        .map(|c| {
            let f = |label: &str, key: &str| fact(block, c, label, key, ctx);
            let mut cells = vec![
                (f("Subject", "subject"), flags(c)?),
                (f("Alternative names", "sans"), String::new()),
                (f("Issuer", "issuer"), String::new()),
            ];
            cells.extend(
                validity_cells(c)
                    .into_iter()
                    .map(|c| (Some(c), String::new())),
            );
            cells.extend([
                (f("Key", "key"), String::new()),
                (f("Signature", "signature_algorithm"), String::new()),
                (f("Serial", "serial"), String::new()),
                (f("SHA-256", "fingerprint_sha256"), String::new()),
                (f("SHA-1", "fingerprint_sha1"), String::new()),
                (f("Policy", "policy"), String::new()),
                (f("OCSP responder", "ocsp_url"), String::new()),
                (f("CA issuers", "ca_issuers_url"), String::new()),
            ]);
            Some(ChainCert {
                name: get(c, "subject")
                    .map_or_else(String::new, |v| common_name(&display_value(v))),
                grid: grid(cells)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    CertificateView {
        validity,
        grid: grid_html,
        pills,
        inferred,
        trust,
        more,
        anchors: members[1..]
            .iter()
            .map(|(_, b, _)| b.id.to_string())
            .collect(),
    }
    .render()
    .ok()
}
