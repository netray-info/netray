//! v2-ip-ui R8 (C1, C6, C7, C8, C12): the IP view's "Network" facts grid, human labels, optional cells.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render};

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

/// An `ip.lookup` block: the address, the nine flags (false), then `extra`.
fn html(extra: Vec<(&str, Value)>) -> String {
    let mut fields = vec![(
        "address".to_string(),
        Value::Addr("192.0.2.10".parse().unwrap()),
    )];
    for flag in [
        "tor",
        "c2",
        "spamhaus",
        "cins",
        "vpn",
        "datacenter",
        "anycast",
        "bot",
        "internal",
    ] {
        fields.push((flag.into(), Value::Bool(false)));
    }
    fields.extend(extra.into_iter().map(|(k, v)| (k.to_string(), v)));
    let block = Block {
        id: BlockId::parse("ip.lookup").unwrap(),
        title: "Lookup".into(),
        shape: Shape::Fields(fields),
    };
    let decorate = decorate(&[]);
    render(
        &block,
        &ViewContext {
            statuses: &[],
            decorate: &decorate,
            feeds: &[],
        },
    )
    .expect("the IP view claims ip.lookup")
}

fn label(l: &str) -> String {
    format!(r#"<span class="facts-grid__label">{l}</span>"#)
}

/// The visible text of the cell labelled `l`, tags stripped; None when no such cell.
fn cell_text(html: &str, l: &str) -> Option<String> {
    let start = html.find(&label(l))? + label(l).len();
    let rest = &html[start..];
    let end = rest
        .find(r#"<div class="facts-grid__cell""#)
        .unwrap_or(rest.len());
    let rest = &rest[..end];
    let rest = &rest[..rest.find("</div>").unwrap_or(rest.len())];
    let mut out = String::new();
    let mut in_tag = false;
    for c in rest.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    Some(out)
}

#[test]
fn network_section_has_human_labels_and_values() {
    let html = html(vec![
        ("asn", text("AS64496")),
        ("org", text("Example Cloud GmbH")),
        ("prefix", text("192.0.2.0/24")),
        ("network_type", text("cloud")),
        ("network_role", text("stub")),
        ("asn_category", text("hosting")),
        ("asn_registered", text("2010-01-01")),
        ("iana_label", text("Documentation (TEST-NET-1)")),
    ]);
    assert!(
        html.contains(">Network<"),
        "heading Network missing in: {html}"
    );
    for (l, v) in [
        ("ASN", "AS64496"),
        ("Organisation", "Example Cloud GmbH"),
        ("Prefix", "192.0.2.0/24"),
        ("Network type", "cloud"),
        ("Network role", "stub"),
        ("ASN category", "hosting"),
        ("ASN registered", "2010-01-01"),
        ("IANA label", "Documentation (TEST-NET-1)"),
    ] {
        let cell = cell_text(&html, l).unwrap_or_else(|| panic!("label {l:?} missing in: {html}"));
        assert_eq!(cell, v, "cell {l:?} in: {html}");
    }
    for raw in [
        "asn",
        "org",
        "prefix",
        "network_type",
        "network_role",
        "asn_category",
        "asn_registered",
        "iana_label",
    ] {
        assert!(
            !html.contains(&label(raw)),
            "raw key {raw:?} as label in: {html}"
        );
    }
}

#[test]
fn optional_network_cells_appear_only_with_their_value() {
    // (extra fields, label, expected cell text or None when the cell must be absent)
    type Case = (
        Vec<(&'static str, Value)>,
        &'static str,
        Option<&'static str>,
    );
    let cases: Vec<Case> = vec![
        (
            vec![
                ("cloud_provider", text("aws")),
                ("cloud_service", text("EC2")),
                ("cloud_region", text("eu-central-1")),
            ],
            "Cloud",
            Some("aws · EC2 · eu-central-1"),
        ),
        (vec![("cloud_provider", text("aws"))], "Cloud", Some("aws")),
        (vec![], "Cloud", None),
        (
            vec![("vpn_provider", text("ExampleVPN"))],
            "VPN provider",
            Some("ExampleVPN"),
        ),
        (vec![], "VPN provider", None),
        (
            vec![("bot_provider", text("examplebot"))],
            "Bot provider",
            Some("examplebot"),
        ),
        (vec![], "Bot provider", None),
        (
            vec![("tcp_port", Value::Int(54321))],
            "TCP source port",
            Some("54321"),
        ),
        (vec![], "TCP source port", None),
    ];
    for (extra, l, want) in cases {
        let html = html(extra);
        assert_eq!(
            cell_text(&html, l).as_deref(),
            want,
            "label {l:?} in: {html}"
        );
    }
}
