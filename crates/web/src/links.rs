//! Cross-links from evidence values to the inspectors (spec v2-inspector-shell, requirement 21):
//! one function over a block's typed values, used by every view that renders a block.

use std::net::IpAddr;

use netray_model::evidence::{Block, BlockId, Shape, Value};
use percent_encoding::utf8_percent_encode;

use crate::evidence::{Badge, Decoration};
use crate::explore::COMPONENT;

/// The IP inspector's page of `addr`.
pub fn ip_href(addr: IpAddr) -> String {
    format!(
        "/inspect/ip?ip={}",
        utf8_percent_encode(&addr.to_string(), COMPONENT)
    )
}

/// The DNS inspector's page of the query `q`.
pub fn dns_query_href(q: &str) -> String {
    format!("/inspect/dns?q={}", utf8_percent_encode(q, COMPONENT))
}

/// The record type a record's data cell carries in its key (`data:<TYPE>`, as the renderer
/// passes it), upper-cased; `None` for any other key.
fn record_type(key: &str) -> Option<String> {
    key.strip_prefix("data:").map(str::to_ascii_uppercase)
}

/// The address a value holds: an `addr` value, or the data of an A or AAAA record, whatever block
/// it sits in (an NS answer carries its glue).
fn address(_block: &BlockId, key: &str, value: &Value) -> Option<IpAddr> {
    match value {
        Value::Addr(addr) => Some(*addr),
        Value::Text(text) if matches!(record_type(key).as_deref(), Some("A" | "AAAA")) => {
            text.trim().parse().ok()
        }
        _ => None,
    }
}

/// The host name a value names: NS, CNAME and MX record data, and `email.mx`'s host and CNAME.
fn host(block: &BlockId, key: &str, value: &Value) -> Option<String> {
    let Value::Text(text) = value else {
        return None;
    };
    let raw = match record_type(key).as_deref() {
        Some("NS" | "CNAME") => text.as_str(),
        Some("MX") => text.split_whitespace().nth(1)?,
        Some(_) => return None,
        None => match (block.module(), block.name(), key) {
            ("email", "mx", "host" | "cname") => text.as_str(),
            _ => return None,
        },
    };
    let host = raw.trim().trim_end_matches('.');
    (!host.is_empty()).then(|| host.to_string())
}

/// The inspector page a value of `block` links to: an address to the IP inspector, a host name
/// to the DNS inspector. `key` is the record field (`data`), the column or the field name.
pub fn value_link(block: &BlockId, key: &str, value: &Value) -> Option<String> {
    address(block, key, value)
        .map(ip_href)
        .or_else(|| host(block, key, value).map(|h| dns_query_href(&h)))
}

/// Every address `block` holds, once each, in order.
pub fn addresses(block: &Block) -> Vec<IpAddr> {
    let mut out: Vec<IpAddr> = Vec::new();
    let push = |out: &mut Vec<IpAddr>, addr: IpAddr| {
        if !out.contains(&addr) {
            out.push(addr);
        }
    };
    let mut values: Vec<(&str, &Value)> = Vec::new();
    match &block.shape {
        Shape::Records(records) => {
            for r in records {
                if let Some(addr) = address(&block.id, &data_key(&r.rtype), &r.data) {
                    push(&mut out, addr);
                }
                for v in [&r.owner, &r.rtype, &r.ttl] {
                    if let Value::Addr(addr) = v {
                        push(&mut out, *addr);
                    }
                }
            }
        }
        Shape::Table { columns, rows, .. } => {
            for row in rows {
                values.extend(columns.iter().map(String::as_str).zip(row));
            }
        }
        Shape::Chain(links) => {
            for link in links {
                values.extend(link.iter().map(|(k, v)| (k.as_str(), v)));
            }
        }
        Shape::Fields(pairs) => values.extend(pairs.iter().map(|(k, v)| (k.as_str(), v))),
        Shape::Tree(_) | Shape::Text(_) => {}
    }
    for (key, value) in values {
        if let Some(addr) = address(&block.id, key, value) {
            push(&mut out, addr);
        }
    }
    out
}

/// The key of a record's data cell: `data:<TYPE>`, so a link follows the row's record type.
pub fn data_key(rtype: &Value) -> String {
    format!("data:{}", crate::evidence::format_value(rtype))
}

/// The decoration of a value: its link and, for an address, the badges `badges` holds for it.
pub fn decorate(
    badges: &[(IpAddr, Vec<Badge>)],
) -> impl Fn(&BlockId, &str, &Value) -> Option<Decoration> + '_ {
    move |block, key, value| {
        let href = value_link(block, key, value);
        let badges = address(block, key, value)
            .and_then(|addr| badges.iter().find(|(a, _)| *a == addr))
            .map(|(_, b)| b.clone())
            .unwrap_or_default();
        (href.is_some() || !badges.is_empty()).then_some(Decoration { href, badges })
    }
}
