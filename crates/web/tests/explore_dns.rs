//! Explore links for DNS records and the view link (V2 spec v2-inspectors, requirement 8,
//! Phase 3): `block_links` of a `dns.<type>` block and `view_link` for all five protocols.
use netray_model::Protocol;
use netray_model::evidence::{Block, BlockId, Record, Shape, Value};
use netray_web::explore::{block_links, view_link};

fn record(owner: &str, rtype: &str, data: &str) -> Record {
    Record {
        owner: Value::Text(owner.into()),
        rtype: Value::Text(rtype.into()),
        ttl: Value::Int(300),
        data: Value::Text(data.into()),
    }
}

fn block(id: &str, records: Vec<Record>) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: id.into(),
        shape: Shape::Records(records),
    }
}

fn hrefs(block: &Block) -> Vec<String> {
    block_links(block, "example.com")
        .into_iter()
        .map(|l| l.href)
        .collect()
}

#[test]
fn dns_mx_records_with_one_owner_link_to_owner_and_type() {
    let mx = block(
        "dns.mx",
        vec![
            record("example.com", "MX", "10 mx1.example.com"),
            record("example.com", "MX", "20 mx2.example.com"),
        ],
    );
    assert_eq!(hrefs(&mx), vec!["/inspect/dns?q=example.com%20MX"]);
}

#[test]
fn dns_a_records_with_two_owners_link_to_the_first_owner() {
    let a = block(
        "dns.a",
        vec![
            record("example.com", "A", "192.0.2.1"),
            record("www.example.com", "A", "192.0.2.2"),
        ],
    );
    assert_eq!(hrefs(&a), vec!["/inspect/dns?q=example.com"]);
}

#[test]
fn dns_block_without_records_yields_no_link() {
    assert!(hrefs(&block("dns.a", vec![])).is_empty());
}

#[test]
fn view_link_names_the_tool_and_encoded_query_per_protocol() {
    let cases = [
        (Protocol::Dns, "/inspect/dns?q=example.com"),
        (Protocol::Tls, "/inspect/tls?h=example.com:443"),
        (
            Protocol::Http,
            "/inspect/http?url=https%3A%2F%2Fexample.com%2F",
        ),
        (Protocol::Email, "/inspect/dns?q=example.com%20MX"),
        (Protocol::Ip, "/inspect/dns?q=example.com%20A"),
    ];
    for (protocol, want) in cases {
        let link = view_link(protocol, "example.com");
        assert_eq!(link.href, want, "{protocol:?}");
        assert!(link.href.starts_with("/inspect/"), "{protocol:?}");
    }
}
