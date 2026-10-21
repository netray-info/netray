//! v2-ip-ui Phase 1 (R5, C5, C17): the IP card is the view of `ip.lookup`, anchored
//! `ev-ip.lookup`, shows every value of the block, and does not claim a block with an unknown key.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::evidence::display_value;
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render};

/// askama's escaping of a text value.
fn escaped(text: &str) -> String {
    // askama 0.16's HTML escaping.
    text.replace('&', "&#38;")
        .replace('<', "&#60;")
        .replace('>', "&#62;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

/// The nine flags with the badge label each shows when true.
const FLAGS: [(&str, &str); 9] = [
    ("tor", "Tor"),
    ("c2", "C2"),
    ("spamhaus", "Spamhaus"),
    ("cins", "CINS"),
    ("vpn", "VPN"),
    ("datacenter", "datacenter"),
    ("anycast", "anycast"),
    ("bot", "bot"),
    ("internal", "internal"),
];

fn lookup_fields() -> Vec<(String, Value)> {
    let mut fields: Vec<(String, Value)> = vec![
        ("address".into(), Value::Addr("192.0.2.10".parse().unwrap())),
        ("hostname".into(), text("host-1.example.com")),
        ("tcp_port".into(), Value::Int(54321)),
        ("asn".into(), text("AS64496")),
        ("org".into(), text("Example Networks Ltd")),
        ("prefix".into(), text("192.0.2.0/24")),
        ("network_type".into(), text("hosting")),
        ("infra_type".into(), text("cloud-infra")),
        ("asn_category".into(), text("content-delivery")),
        ("network_role".into(), text("transit-provider")),
        ("asn_registered".into(), text("2009-03-17")),
        ("iana_label".into(), text("Documentation (TEST-NET-1)")),
        ("cloud_provider".into(), text("Example Cloud")),
        ("cloud_service".into(), text("Example Compute")),
        ("cloud_region".into(), text("example-north-1")),
        ("vpn_provider".into(), text("Example VPN")),
        ("bot_provider".into(), text("Example Crawler")),
    ];
    for (key, _) in FLAGS {
        fields.push((key.into(), Value::Bool(true)));
    }
    fields.extend([
        ("city".into(), text("Exampleburg")),
        ("region".into(), text("Example State")),
        ("region_code".into(), text("ES")),
        ("country".into(), text("Exampleland")),
        ("country_iso".into(), text("XL")),
        ("postal_code".into(), text("99001")),
        ("eu".into(), Value::Bool(true)),
        ("latitude".into(), text("52.52")),
        ("longitude".into(), text("13.4")),
        ("accuracy_km".into(), Value::Int(20)),
        ("timezone".into(), text("Europe/Example")),
        ("continent".into(), text("Exampleia")),
        ("continent_code".into(), text("XA")),
        ("registered_country".into(), text("Registryland")),
        ("registered_country_iso".into(), text("RL")),
        (
            "ua_raw".into(),
            text("ExampleAgent/1.0 (X11; <Test> & 'q')"),
        ),
        ("ua_browser".into(), text("ExampleBrowser")),
        ("ua_browser_version".into(), text("118.4")),
        ("ua_os".into(), text("ExampleOS")),
        ("ua_os_version".into(), text("14.2")),
        ("ua_device".into(), text("ExamplePhone")),
    ]);
    fields
}

fn block(fields: Vec<(String, Value)>) -> Block {
    Block {
        id: BlockId::parse("ip.lookup").unwrap(),
        title: "Lookup".into(),
        shape: Shape::Fields(fields),
    }
}

#[test]
fn c17_ip_card_shows_every_value_and_refuses_an_unknown_key() {
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };

    let fields = lookup_fields();
    let html = render(&block(fields.clone()), &ctx)
        .expect("the view must claim an ip.lookup block of registered keys");
    assert!(
        html.contains("id=\"ev-ip.lookup\""),
        "anchor missing in: {html}"
    );

    for (key, value) in &fields {
        let needle = match value {
            Value::Bool(_) => continue,
            other => escaped(&display_value(other)),
        };
        assert!(
            html.contains(needle.as_str()),
            "{key}: {needle:?} missing in: {html}"
        );
    }
    for (_, label) in FLAGS.iter().chain(&[("eu", "EU")]) {
        let needle = format!(">{label}</span>");
        assert!(html.contains(&needle), "badge {label:?} missing in: {html}");
    }

    let mut unknown = lookup_fields();
    unknown.push(("unknown_key".into(), text("x")));
    assert!(
        render(&block(unknown), &ctx).is_none(),
        "a block with an unknown key must not be claimed"
    );
}
