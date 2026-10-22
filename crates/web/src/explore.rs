//! Explore-live links: from a view or one of its evidence blocks to the matching inspector page.

use netray_model::Protocol;
use netray_model::evidence::{Block, Shape, Value};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::evidence::format_value;

pub(crate) const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
const QUERY: &AsciiSet = &COMPONENT.remove(b':');

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExploreLink {
    pub label: String,
    pub href: String,
}

impl ExploreLink {
    /// The link named by its inspector and the query it opens: "DNS · example.com MX".
    pub fn target(&self) -> String {
        let rest = self.href.strip_prefix("/inspect/").unwrap_or(&self.href);
        let (tool, query) = rest.split_once('?').unwrap_or((rest, ""));
        let value = query.split_once('=').map_or("", |(_, v)| v);
        let value = percent_encoding::percent_decode_str(value).decode_utf8_lossy();
        let name = match tool {
            "email" => "Email".to_string(),
            other => other.to_uppercase(),
        };
        format!("{name} · {value}")
    }
}

fn link(label: impl Into<String>, tool: &str, field: &str, value: &str) -> ExploreLink {
    let set = if field == "url" { COMPONENT } else { QUERY };
    ExploreLink {
        label: label.into(),
        href: format!(
            "/inspect/{tool}?{field}={}",
            utf8_percent_encode(value, set)
        ),
    }
}

pub fn view_link(protocol: Protocol, domain: &str) -> ExploreLink {
    let label = "Explore live";
    match protocol {
        Protocol::Dns => link(label, "dns", "q", domain),
        Protocol::Tls => link(label, "tls", "h", &format!("{domain}:443")),
        Protocol::Http => link(label, "http", "url", &format!("https://{domain}/")),
        Protocol::Email => link(label, "dns", "q", &format!("{domain} MX")),
        Protocol::Ip => link(label, "dns", "q", &format!("{domain} A")),
    }
}

fn column_values<'a>(shape: &'a Shape, column: &str) -> Vec<&'a Value> {
    let Shape::Table { columns, rows, .. } = shape else {
        return Vec::new();
    };
    let Some(index) = columns.iter().position(|c| c == column) else {
        return Vec::new();
    };
    rows.iter().filter_map(|row| row.get(index)).collect()
}

/// Links from one evidence block to the inspector pages that show it live.
///
/// A `dns.<type>` records block links to `/inspect/dns?q=<owner> <TYPE>`, `TYPE` being the block
/// name upper-cased. The owner is the records' shared owner, or `domain` when the owners differ
/// (a CNAME chain) or there are no records. An `email.mx` row whose host is empty after dropping
/// trailing dots (null MX, RFC 7505) gets no link. A `tls.addresses@<port>` block links
/// `<address>:<port>`; an unqualified one links port 443.
pub fn block_links(block: &Block, domain: &str) -> Vec<ExploreLink> {
    match (block.id.module(), block.id.name()) {
        ("dns", name) => {
            let Shape::Records(records) = &block.shape else {
                return Vec::new();
            };
            let owner = records.first().map(|r| format_value(&r.owner));
            let owner = match owner {
                Some(first) if records.iter().all(|r| format_value(&r.owner) == first) => first,
                _ => domain.to_string(),
            };
            let query = format!("{owner} {}", name.to_uppercase());
            vec![link(owner, "dns", "q", &query)]
        }
        ("tls", "addresses") => {
            let port = block
                .id
                .qualifier()
                .and_then(|q| q.parse::<u16>().ok())
                .unwrap_or(443);
            column_values(&block.shape, "address")
                .into_iter()
                .map(|value| {
                    let address = format_value(value);
                    let host = match value {
                        Value::Addr(addr) if addr.is_ipv6() => format!("[{address}]"),
                        _ => address.clone(),
                    };
                    link(address, "tls", "h", &format!("{host}:{port}"))
                })
                .collect()
        }
        ("email", "mx") => column_values(&block.shape, "host")
            .into_iter()
            .filter_map(|value| {
                let host = format_value(value);
                let host = host.trim_end_matches('.');
                (!host.is_empty() && host.is_ascii())
                    .then(|| link(host, "tls", "h", &format!("{host}:25")))
            })
            .collect(),
        ("email", "dmarc") => vec![link(
            "Explore live",
            "dns",
            "q",
            &format!("_dmarc.{domain} TXT"),
        )],
        ("email", "mta_sts") => vec![link(
            "Explore live",
            "dns",
            "q",
            &format!("_mta-sts.{domain} TXT"),
        )],
        ("ip", "addresses") => column_values(&block.shape, "address")
            .into_iter()
            .map(|value| {
                let address = format_value(value);
                link(address.clone(), "ip", "ip", &address)
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub fn view_links(protocol: Protocol, domain: &str, blocks: &[Block]) -> Vec<ExploreLink> {
    let mut links = vec![view_link(protocol, domain)];
    links.extend(
        blocks
            .iter()
            .filter(|b| b.id.module() == protocol.as_str())
            .flat_map(|b| block_links(b, domain)),
    );
    links
}
