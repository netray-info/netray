//! V2 spec v2-inspectors, requirement 8 (Phase 3): the email view's explore links.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::explore::block_links;

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn block(id: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: "Title".into(),
        shape,
    }
}

fn mx_block(hosts: &[&str]) -> Block {
    block(
        "email.mx",
        Shape::Table {
            key: "host".into(),
            columns: vec!["host".into(), "preference".into()],
            rows: hosts
                .iter()
                .enumerate()
                .map(|(i, h)| vec![text(h), Value::Int(10 * (i as i64 + 1))])
                .collect(),
        },
    )
}

fn hrefs(block: &Block) -> Vec<String> {
    block_links(block, "example.com")
        .into_iter()
        .map(|l| l.href)
        .collect()
}

#[test]
fn mx_row_links_to_tls_on_port_25() {
    assert_eq!(
        hrefs(&mx_block(&["mx.example.com"])),
        ["/inspect/tls?h=mx.example.com:25"]
    );
}

#[test]
fn two_mx_rows_link_in_row_order() {
    assert_eq!(
        hrefs(&mx_block(&["mx1.example.com", "mx2.example.com"])),
        [
            "/inspect/tls?h=mx1.example.com:25",
            "/inspect/tls?h=mx2.example.com:25"
        ]
    );
}

#[test]
fn mx_trailing_dot_is_dropped() {
    assert_eq!(
        hrefs(&mx_block(&["mx2.example.com."])),
        ["/inspect/tls?h=mx2.example.com:25"]
    );
}

#[test]
fn dmarc_and_mta_sts_link_to_their_txt_names() {
    let fields = Shape::Fields(vec![("policy".into(), text("reject"))]);
    assert_eq!(
        hrefs(&block("email.dmarc", fields.clone())),
        ["/inspect/dns?q=_dmarc.example.com%20TXT"]
    );
    assert_eq!(
        hrefs(&block("email.mta_sts", fields)),
        ["/inspect/dns?q=_mta-sts.example.com%20TXT"]
    );
}

#[test]
fn spf_block_has_no_link() {
    let b = block(
        "email.spf",
        Shape::Fields(vec![("record".into(), text("v=spf1 -all"))]),
    );
    assert!(hrefs(&b).is_empty());
}

#[test]
fn null_mx_row_has_no_link() {
    // RFC 7505 null MX: the host is ".", which names no server to inspect.
    assert!(hrefs(&mx_block(&["."])).is_empty());
}
