//! v2-tls-ui Phase 1 (C1, C10, C12, C13): the TLS port view's seam, sections and anchors.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::pages::views::{ViewContext, render_with};
use std::net::IpAddr;

fn addr(s: &str) -> Value {
    Value::Addr(s.parse::<IpAddr>().unwrap())
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn addresses(id: &str, ips: &[&str]) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
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

fn chain(id: &str) -> Block {
    let leaf: Vec<(String, Value)> = vec![
        ("position", Value::Int(0)),
        ("subject", text("CN=example.com")),
        ("issuer", text("CN=Example Issuing CA")),
        ("sans", text("example.com")),
        ("not_before", Value::Time("2026-10-07T00:00:00Z".into())),
        ("not_after", Value::Time("2027-01-06T00:00:00Z".into())),
        ("days_remaining", Value::Int(89)),
        ("key", text("ECDSA P-256")),
        ("signature_algorithm", text("ecdsa-with-SHA256")),
        (
            "fingerprint_sha256",
            text("4F:9A:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:01:02:03:04:05:06:07:08:09:0A:0B:0C:0D:0E:0F:E1"),
        ),
        ("self_signed", Value::Bool(false)),
        ("expired", Value::Bool(false)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    Block {
        id: BlockId::parse(id).unwrap(),
        title: "Certificate chain".into(),
        shape: Shape::Chain(vec![leaf]),
    }
}

fn handshake(id: &str) -> Block {
    let fields: Vec<(String, Value)> = vec![
        ("version", text("TLSv1.3")),
        ("cipher_suite", text("TLS_AES_256_GCM_SHA384")),
        ("alpn", text("h2")),
        ("handshake_ms", Value::Duration(22)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    Block {
        id: BlockId::parse(id).unwrap(),
        title: "Handshake".into(),
        shape: Shape::Fields(fields),
    }
}

fn render_one(block: &Block, siblings: &[Block]) -> Option<String> {
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    render_with(block, &ctx, siblings)
}

fn count(html: &str, needle: &str) -> usize {
    html.matches(needle).count()
}

fn two_address_port(qualified: bool) -> Vec<Block> {
    let (a_id, c1, c2, h1, h2) = if qualified {
        (
            "tls.addresses@443",
            "tls.chain@192.0.2.1:443",
            "tls.chain@192.0.2.2:443",
            "tls.handshake@192.0.2.1:443",
            "tls.handshake@192.0.2.2:443",
        )
    } else {
        (
            "tls.addresses",
            "tls.chain@192.0.2.1:443",
            "tls.chain@192.0.2.2:443",
            "tls.handshake@192.0.2.1:443",
            "tls.handshake@192.0.2.2:443",
        )
    };
    vec![
        addresses(a_id, &["192.0.2.1", "192.0.2.2"]),
        chain(c1),
        chain(c2),
        handshake(h1),
        handshake(h2),
    ]
}

#[test]
fn c1_c10_c12_the_port_view_renders_once_at_addresses_and_siblings_render_nothing() {
    let blocks = two_address_port(false);
    let html = render_one(&blocks[0], &blocks).expect("addresses block is claimed");

    assert_eq!(
        count(&html, r#"protocol-view__title">Addresses<"#),
        1,
        "{html}"
    );
    assert_eq!(
        count(&html, r#"protocol-view__title">Certificate<"#),
        1,
        "{html}"
    );
    assert_eq!(count(&html, r#"data-tone="pass">same</span>"#), 2, "{html}");
    assert_eq!(count(&html, "differs"), 0, "{html}");

    assert!(html.contains(r#"id="ev-tls.addresses""#), "{html}");
    for taken in &blocks[1..] {
        assert_eq!(
            render_one(taken, &blocks).as_deref(),
            Some(""),
            "{} renders nothing of its own",
            taken.id
        );
        let anchor = format!(r#"id="ev-{}""#, taken.id);
        assert_eq!(count(&html, &anchor), 1, "{anchor}: {html}");
    }
}

#[test]
fn c12_qualified_ids_carry_the_qualified_anchor() {
    let blocks = two_address_port(true);
    let html = render_one(&blocks[0], &blocks).expect("addresses block is claimed");
    assert_eq!(count(&html, r#"id="ev-tls.addresses@443""#), 1, "{html}");
}

#[test]
fn c13_a_chain_with_an_unknown_key_is_not_taken_and_the_table_still_renders() {
    let mut blocks = two_address_port(false);
    if let Shape::Chain(certs) = &mut blocks[1].shape {
        certs[0].push(("unheard_of_key".into(), text("x")));
    }
    let odd = blocks[1].clone();
    assert_eq!(render_one(&odd, &blocks), None);
    let html = render_one(&blocks[0], &blocks).expect("addresses block is claimed");
    assert!(html.contains(r#"<table class="tls-addresses">"#), "{html}");
    assert_eq!(count(&html, &format!(r#"id="ev-{}""#, odd.id)), 0, "{html}");
}
