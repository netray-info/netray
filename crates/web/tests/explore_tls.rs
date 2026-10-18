//! V2 spec v2-inspectors, requirement 8 (Phase 3): the explore-live links of the TLS view, the
//! view's own link plus one per `tls.addresses` row, none for other blocks or other modules.
use netray_model::Protocol;
use netray_model::evidence::{Block, BlockId, Record, Shape, Value};
use netray_web::explore::{block_links, view_links};

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn addr(s: &str) -> Value {
    Value::Addr(s.parse().unwrap())
}

fn block(id: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: "Title".into(),
        shape,
    }
}

fn addresses() -> Block {
    block(
        "tls.addresses",
        Shape::Table {
            key: "address".into(),
            columns: vec![
                "address".into(),
                "port".into(),
                "outcome".into(),
                "message".into(),
            ],
            rows: vec![
                vec![addr("192.0.2.1"), Value::Int(443), text("ok"), text("")],
                vec![addr("2001:db8::1"), Value::Int(443), text("ok"), text("")],
            ],
        },
    )
}

#[test]
fn tls_addresses_rows_link_to_the_inspector_with_ipv6_bracketed() {
    let hrefs: Vec<String> = block_links(&addresses(), "example.com")
        .into_iter()
        .map(|l| l.href)
        .collect();
    assert_eq!(
        hrefs,
        vec![
            "/inspect/tls?h=192.0.2.1:443".to_string(),
            "/inspect/tls?h=%5B2001:db8::1%5D:443".to_string(),
        ]
    );
}

#[test]
fn tls_view_links_hold_the_view_link_then_one_per_address_row_only() {
    let chain = block(
        "tls.chain",
        Shape::Chain(vec![vec![("subject".into(), text("example.com"))]]),
    );
    let caa = block(
        "dns.caa",
        Shape::Records(vec![Record {
            owner: text("example.com"),
            rtype: text("CAA"),
            ttl: Value::Int(300),
            data: text("0 issue \"ca.example.com\""),
        }]),
    );
    let blocks = vec![addresses(), chain, caa];
    let hrefs: Vec<String> = view_links(Protocol::Tls, "example.com", &blocks)
        .into_iter()
        .map(|l| l.href)
        .collect();
    assert_eq!(
        hrefs,
        vec![
            "/inspect/tls?h=example.com:443".to_string(),
            "/inspect/tls?h=192.0.2.1:443".to_string(),
            "/inspect/tls?h=%5B2001:db8::1%5D:443".to_string(),
        ]
    );
}
