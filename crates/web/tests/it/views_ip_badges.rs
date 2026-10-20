//! v2-ip-ui R7 (C7, C19): the IP view's badge row, one chip per fact, toned by kind.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render};

const FLAGS: [&str; 9] = [
    "tor",
    "c2",
    "spamhaus",
    "cins",
    "vpn",
    "datacenter",
    "anycast",
    "bot",
    "internal",
];

/// An `ip.lookup` block; `eu` is omitted when `None`, `infra` likewise.
fn lookup(addr: &str, eu: Option<bool>, infra: Option<&str>, on: &[&str]) -> Block {
    let mut fields = vec![("address".to_string(), Value::Addr(addr.parse().unwrap()))];
    if let Some(eu) = eu {
        fields.push(("eu".into(), Value::Bool(eu)));
    }
    if let Some(infra) = infra {
        fields.push(("infra_type".into(), Value::Text(infra.into())));
    }
    for flag in FLAGS {
        fields.push((flag.into(), Value::Bool(on.contains(&flag))));
    }
    Block {
        id: BlockId::parse("ip.lookup").unwrap(),
        title: "Lookup".into(),
        shape: Shape::Fields(fields),
    }
}

fn html(block: &Block) -> String {
    let decorate = decorate(&[]);
    render(
        block,
        &ViewContext {
            statuses: &[],
            decorate: &decorate,
            feeds: &[],
        },
    )
    .expect("the IP view claims ip.lookup")
}

fn badge(label: &str, tone: &str) -> String {
    format!(r#"<span class="ip-badge" data-tone="{tone}">{label}</span>"#)
}

fn badge_count(html: &str) -> usize {
    html.matches(r#"class="ip-badge""#).count()
}

#[test]
fn badges_read_family_eu_infra_and_true_flags() {
    let html = html(&lookup("192.0.2.10", Some(true), Some("cloud"), &["tor", "vpn"]));
    for (label, tone) in [("Tor", "fail"), ("VPN", "warn")] {
        assert!(html.contains(&badge(label, tone)), "{label}: {html}");
    }
    for label in ["IPv4", "EU", "cloud"] {
        assert!(html.contains(&format!(">{label}</span>")), "{label}: {html}");
    }
    for absent in ["C2", "Spamhaus", "CINS", "IPv6", "non-EU"] {
        assert!(!html.contains(&format!(">{absent}</span>")), "{absent}: {html}");
    }
    assert_eq!(badge_count(&html), 5, "{html}");
}

#[test]
fn badges_family_and_eu_variants() {
    let v6 = html(&lookup("2001:db8::1", Some(false), None, &[]));
    assert!(v6.contains(">IPv6</span>") && v6.contains(">non-EU</span>"), "{v6}");
    assert!(!v6.contains(">IPv4</span>"), "{v6}");
    assert_eq!(badge_count(&v6), 2, "{v6}");

    let unknown = html(&lookup("192.0.2.10", None, None, &[]));
    assert!(unknown.contains(">IPv4</span>"), "{unknown}");
    assert!(!unknown.contains(">EU</span>") && !unknown.contains(">non-EU</span>"), "{unknown}");
    assert_eq!(badge_count(&unknown), 1, "{unknown}");
}

#[test]
fn each_flag_has_its_label_and_tone() {
    let table = [
        ("tor", "Tor", "fail"),
        ("c2", "C2", "fail"),
        ("spamhaus", "Spamhaus", "fail"),
        ("cins", "CINS", "fail"),
        ("vpn", "VPN", "warn"),
        ("datacenter", "datacenter", "muted"),
        ("anycast", "anycast", "muted"),
        ("bot", "bot", "muted"),
        ("internal", "internal", "muted"),
    ];
    assert_eq!(table.len(), FLAGS.len());
    for (flag, label, tone) in table {
        let html = html(&lookup("192.0.2.10", None, None, &[flag]));
        assert!(html.contains(&badge(label, tone)), "{flag}: {html}");
        assert_eq!(badge_count(&html), 2, "{flag}: {html}");
    }
}
