//! V2 spec v2-tls-backend, R10 (C15): a `tls.addresses@<port>` row links to the inspector with
//! that port, a plain `tls.addresses` row with 443.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::explore::block_links;

fn addresses(id: &str, address: &str, port: i64) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: "Title".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: vec![
                "address".into(),
                "port".into(),
                "outcome".into(),
                "message".into(),
            ],
            rows: vec![vec![
                Value::Addr(address.parse().unwrap()),
                Value::Int(port),
                Value::Text("ok".into()),
                Value::Text("".into()),
            ]],
        },
    }
}

fn hrefs(block: &Block) -> Vec<String> {
    block_links(block, "example.com")
        .into_iter()
        .map(|l| l.href)
        .collect()
}

#[test]
fn ported_addresses_block_links_with_its_port() {
    assert_eq!(
        hrefs(&addresses("tls.addresses@465", "192.0.2.10", 465)),
        vec!["/inspect/tls?h=192.0.2.10:465".to_string()]
    );
}

#[test]
fn plain_addresses_block_links_with_443() {
    assert_eq!(
        hrefs(&addresses("tls.addresses", "192.0.2.10", 443)),
        vec!["/inspect/tls?h=192.0.2.10:443".to_string()]
    );
}

#[test]
fn ported_ipv6_address_is_bracketed() {
    assert_eq!(
        hrefs(&addresses("tls.addresses@465", "2001:db8::1", 465)),
        vec!["/inspect/tls?h=%5B2001:db8::1%5D:465".to_string()]
    );
}
