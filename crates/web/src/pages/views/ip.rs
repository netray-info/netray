//! The IP views.

use askama::Template;
use netray_model::evidence::{Block, Value};
use percent_encoding::utf8_percent_encode;

use super::{ViewContext, claim, claim_tree, frame};
use crate::components::{FactCell, FactsGridView, facts_grid};
use crate::evidence::{BadgeTone, cell, display_value};
use crate::explore::COMPONENT;
use crate::links;

pub(super) fn render(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    if block.id.module() != "ip" || block.id.qualifier().is_some() {
        return None;
    }
    match block.id.name() {
        "lookup" => lookup(block, ctx),
        "headers" => headers(block, ctx),
        _ => None,
    }
}

/// The flags of `ip.lookup` with the badge label and tone each shows when true.
const FLAGS: [(&str, &str, BadgeTone); 9] = [
    ("tor", "Tor", BadgeTone::Fail),
    ("c2", "C2", BadgeTone::Fail),
    ("spamhaus", "Spamhaus", BadgeTone::Fail),
    ("cins", "CINS", BadgeTone::Fail),
    ("vpn", "VPN", BadgeTone::Warn),
    ("datacenter", "datacenter", BadgeTone::Muted),
    ("anycast", "anycast", BadgeTone::Muted),
    ("bot", "bot", BadgeTone::Muted),
    ("internal", "internal", BadgeTone::Muted),
];

const LOOKUP_KEYS: &[&str] = &[
    "address",
    "hostname",
    "tcp_port",
    "asn",
    "org",
    "prefix",
    "network_type",
    "infra_type",
    "asn_category",
    "network_role",
    "asn_registered",
    "iana_label",
    "cloud_provider",
    "cloud_service",
    "cloud_region",
    "vpn_provider",
    "bot_provider",
    "tor",
    "c2",
    "spamhaus",
    "cins",
    "vpn",
    "datacenter",
    "anycast",
    "bot",
    "internal",
    "city",
    "region",
    "region_code",
    "country",
    "country_iso",
    "postal_code",
    "eu",
    "latitude",
    "longitude",
    "accuracy_km",
    "timezone",
    "continent",
    "continent_code",
    "registered_country",
    "registered_country_iso",
    "ua_raw",
    "ua_browser",
    "ua_browser_version",
    "ua_os",
    "ua_os_version",
    "ua_device",
];

struct Host {
    name: String,
    dns_href: String,
    tls_href: String,
}

struct Chip {
    label: String,
    tone: &'static str,
}

#[derive(Template)]
#[template(path = "partials/ip_card.html")]
struct IpCard {
    address: String,
    host: Option<Host>,
    badges: Vec<Chip>,
    grid: Option<String>,
}

/// `ip.lookup` as the address and its host name, a badge row of the facts that are flags and
/// every other value in a facts grid.
fn lookup(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let pairs = claim(block, LOOKUP_KEYS)?;
    let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let address = get("address")?;
    let host = match get("hostname") {
        Some(Value::Text(name)) => Some(Host {
            dns_href: links::dns_query_href(name),
            tls_href: format!(
                "/inspect/tls?h={}",
                utf8_percent_encode(&format!("{name}:443"), COMPONENT)
            ),
            name: name.clone(),
        }),
        _ => None,
    };

    let chip = |label: &str, tone: BadgeTone| Chip {
        label: label.to_string(),
        tone: tone.as_str(),
    };
    let mut badges = Vec::new();
    if let Value::Addr(addr) = address {
        badges.push(chip(
            if addr.is_ipv4() { "IPv4" } else { "IPv6" },
            BadgeTone::Muted,
        ));
    }
    if let Some(Value::Bool(eu)) = get("eu") {
        badges.push(chip(if *eu { "EU" } else { "non-EU" }, BadgeTone::Muted));
    }
    if let Some(infra) = get("infra_type") {
        badges.push(chip(&display_value(infra), BadgeTone::Muted));
    }
    for (key, label, tone) in FLAGS {
        if matches!(get(key), Some(Value::Bool(true))) {
            badges.push(chip(label, tone));
        }
    }

    const SHOWN: [&str; 4] = ["address", "hostname", "eu", "infra_type"];
    let cells: Vec<FactCell> = pairs
        .iter()
        .filter(|(k, _)| !SHOWN.contains(&k.as_str()) && !FLAGS.iter().any(|(f, _, _)| f == k))
        .map(|(k, v)| FactCell {
            label: k.clone(),
            values: vec![cell(&block.id, k, v, ctx.decorate)],
            chips: false,
            tone: None,
        })
        .collect();
    let grid = if cells.is_empty() {
        None
    } else {
        Some(facts_grid(&FactsGridView { cells }).ok()?)
    };

    let body = IpCard {
        address: display_value(address),
        host,
        badges,
        grid,
    }
    .render()
    .ok()?;
    frame(block, "Lookup", &body, ctx)
}

struct Header {
    name: String,
    value: String,
}

#[derive(Template)]
#[template(path = "partials/ip_headers.html")]
struct IpHeaders {
    hops: Vec<String>,
    headers: Vec<Header>,
}

/// `ip.headers` as a closed disclosure of the proxy hops and the request headers.
fn headers(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let root = claim_tree(block, &["forwarded_for"], &["name", "value"])?;
    let hops = root.fields.iter().map(|(_, v)| display_value(v)).collect();
    let headers = root
        .children
        .iter()
        .map(|c| {
            let get = |key: &str| {
                c.fields
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| display_value(v))
                    .unwrap_or_default()
            };
            Header {
                name: get("name"),
                value: get("value"),
            }
        })
        .collect();
    let body = IpHeaders { hops, headers }.render().ok()?;
    frame(block, "Request headers", &body, ctx)
}
