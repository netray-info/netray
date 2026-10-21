//! The IP views.

use askama::Template;
use netray_model::evidence::{Block, Value};
use percent_encoding::utf8_percent_encode;

use super::{ViewContext, claim, claim_tree, frame};
use crate::components::{Cell, FactCell, FactsGridView, facts_grid};
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

fn fact(label: &str, cell: Cell) -> FactCell {
    FactCell {
        label: label.to_string(),
        values: vec![cell],
        chips: false,
        tone: None,
    }
}

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
    network: Option<String>,
    location: Option<String>,
    map_href: Option<String>,
    user_agent: Option<String>,
    ua_raw: Option<String>,
}

/// `ip.lookup` as the address and its host name, a badge row of the facts that are flags and
/// the rest in Network, Location and User agent facts grids.
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

    let one = |label: &str, key: &str| -> Option<FactCell> {
        get(key).map(|v| fact(label, cell(&block.id, key, v, ctx.decorate)))
    };
    let joined = |label: &str, keys: &[&str], sep: &str| -> Option<FactCell> {
        let parts: Vec<(&str, &Value)> = keys
            .iter()
            .filter_map(|k| get(k).map(|v| (*k, v)))
            .collect();
        let (key, first) = *parts.first()?;
        let text = parts
            .iter()
            .map(|(_, v)| display_value(v))
            .collect::<Vec<_>>()
            .join(sep);
        Some(fact(label, super::cell_with(block, key, first, ctx, text)))
    };
    let coded = |label: &str, name: &str, code: &str| -> Option<FactCell> {
        match (get(name), get(code)) {
            (Some(n), Some(c)) => Some(fact(
                label,
                super::cell_with(
                    block,
                    name,
                    n,
                    ctx,
                    format!("{} ({})", display_value(n), display_value(c)),
                ),
            )),
            (Some(_), None) => one(label, name),
            (None, Some(_)) => one(label, code),
            (None, None) => None,
        }
    };

    let network: Vec<FactCell> = [
        one("ASN", "asn"),
        one("Organisation", "org"),
        one("Prefix", "prefix"),
        one("Network type", "network_type"),
        one("Network role", "network_role"),
        one("ASN category", "asn_category"),
        one("ASN registered", "asn_registered"),
        one("IANA label", "iana_label"),
        joined(
            "Cloud",
            &["cloud_provider", "cloud_service", "cloud_region"],
            " · ",
        ),
        one("VPN provider", "vpn_provider"),
        one("Bot provider", "bot_provider"),
        one("TCP source port", "tcp_port"),
    ]
    .into_iter()
    .flatten()
    .collect();

    let accuracy = get("accuracy_km").map(|v| format!("± {} km", display_value(v)));
    let coordinates = match (get("latitude"), get("longitude")) {
        (Some(lat), Some(lon)) => {
            let mut text = format!("{}, {}", display_value(lat), display_value(lon));
            if let Some(acc) = &accuracy {
                text.push(' ');
                text.push_str(acc);
            }
            vec![fact(
                "Coordinates",
                super::cell_with(block, "latitude", lat, ctx, text),
            )]
        }
        _ => {
            let mut cells: Vec<FactCell> =
                [one("Latitude", "latitude"), one("Longitude", "longitude")]
                    .into_iter()
                    .flatten()
                    .collect();
            if let (Some(v), Some(text)) = (get("accuracy_km"), accuracy) {
                cells.push(fact(
                    "Accuracy",
                    super::cell_with(block, "accuracy_km", v, ctx, text),
                ));
            }
            cells
        }
    };
    let country = coded("Country", "country", "country_iso");
    let registered = coded(
        "Registered country",
        "registered_country",
        "registered_country_iso",
    )
    .filter(|r| {
        country
            .as_ref()
            .is_none_or(|c| c.values[0].text != r.values[0].text)
    });
    let mut location: Vec<FactCell> = [
        one("City", "city"),
        coded("Region", "region", "region_code"),
        country,
        coded("Continent", "continent", "continent_code"),
        one("Postal code", "postal_code"),
    ]
    .into_iter()
    .flatten()
    .collect();
    location.extend(coordinates);
    location.extend(
        [one("Timezone", "timezone"), registered]
            .into_iter()
            .flatten(),
    );

    let map_href = match (get("latitude"), get("longitude")) {
        (Some(lat), Some(lon)) => {
            let (lat, lon) = (display_value(lat), display_value(lon));
            let (lat, lon) = (
                utf8_percent_encode(&lat, COMPONENT).to_string(),
                utf8_percent_encode(&lon, COMPONENT).to_string(),
            );
            Some(format!(
                "https://www.openstreetmap.org/?mlat={lat}&mlon={lon}#map=10/{lat}/{lon}"
            ))
        }
        _ => None,
    };

    let ua_raw = get("ua_raw").map(display_value);
    let user_agent: Vec<FactCell> = [
        joined("Browser", &["ua_browser", "ua_browser_version"], " "),
        joined("Operating system", &["ua_os", "ua_os_version"], " "),
        one("Device", "ua_device"),
        one("Raw header", "ua_raw"),
    ]
    .into_iter()
    .flatten()
    .collect();

    let section = |cells: Vec<FactCell>| -> Option<Option<String>> {
        if cells.is_empty() {
            Some(None)
        } else {
            Some(Some(facts_grid(&FactsGridView { cells }).ok()?))
        }
    };
    let network = section(network)?;
    let location = section(location)?;
    let user_agent = section(user_agent)?;

    let body = IpCard {
        address: display_value(address),
        host,
        badges,
        network,
        location,
        map_href,
        user_agent,
        ua_raw,
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
