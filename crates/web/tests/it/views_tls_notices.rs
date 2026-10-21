//! v2-tls-ui Phase 2 (C2, C10): the notices banner above the Addresses section.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::pages::views::{ViewContext, render_with};
use std::net::IpAddr;

fn addr(s: &str) -> Value {
    Value::Addr(s.parse::<IpAddr>().unwrap())
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn addresses(ips: &[&str]) -> Block {
    Block {
        id: BlockId::parse("tls.addresses").unwrap(),
        title: "Addresses".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: ["address", "port", "outcome", "message"]
                .map(String::from)
                .to_vec(),
            rows: ips
                .iter()
                .map(|ip| vec![addr(ip), Value::Int(443), text("ok"), text("")])
                .collect(),
        },
    }
}

fn notices(warnings: &[&str], skipped: &[&str]) -> Block {
    let mut fields: Vec<(String, Value)> = Vec::new();
    for w in warnings {
        fields.push(("warning".into(), text(w)));
    }
    for s in skipped {
        fields.push(("skipped_address".into(), addr(s)));
    }
    Block {
        id: BlockId::parse("tls.notices").unwrap(),
        title: "Notices".into(),
        shape: Shape::Fields(fields),
    }
}

fn render_at_addresses(blocks: &[Block]) -> String {
    let decorate = netray_web::links::decorate(&[]);
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    render_with(&blocks[0], &ctx, blocks).expect("addresses block is claimed")
}

const FOUR: [&str; 4] = ["192.0.2.1", "192.0.2.2", "192.0.2.3", "192.0.2.4"];

#[test]
fn c2_c10_the_banner_names_the_skipped_addresses_and_the_warning_before_the_addresses() {
    let blocks = vec![
        addresses(&FOUR),
        notices(
            &["one resolver answer was ignored"],
            &["192.0.2.9", "192.0.2.10"],
        ),
    ];
    let html = render_at_addresses(&blocks);

    assert!(html.contains(r#"<div class="tls-notices""#), "{html}");
    assert!(
        html.contains(
            "2 addresses not inspected: the inspector checks at most 4 addresses per host"
        ),
        "{html}"
    );
    assert!(html.contains("192.0.2.9"), "{html}");
    assert!(html.contains("192.0.2.10"), "{html}");
    assert!(
        html.contains(r#"href="/inspect/ip?ip=192.0.2.9""#),
        "{html}"
    );
    assert!(html.contains("one resolver answer was ignored"), "{html}");

    let banner = html.find(r#"<div class="tls-notices""#).unwrap();
    let section = html.find(r#"protocol-view__title">Addresses<"#).unwrap();
    assert!(
        banner < section,
        "banner precedes the Addresses section: {html}"
    );

    let decorate = netray_web::links::decorate(&[]);
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    assert_eq!(
        render_with(&blocks[1], &ctx, &blocks).as_deref(),
        Some(""),
        "tls.notices renders nothing of its own"
    );
}

#[test]
fn c10_no_notices_means_no_banner() {
    let html = render_at_addresses(&[addresses(&FOUR)]);
    assert!(!html.contains("tls-notices"), "{html}");
}

#[test]
fn c10_a_warning_alone_shows_without_the_not_inspected_line() {
    let blocks = vec![addresses(&FOUR), notices(&["only a warning"], &[])];
    let html = render_at_addresses(&blocks);
    assert!(html.contains(r#"<div class="tls-notices""#), "{html}");
    assert!(html.contains("only a warning"), "{html}");
    assert!(!html.contains("not inspected"), "{html}");
}
