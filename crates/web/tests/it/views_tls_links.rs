//! v2-tls-ui Phase 1 review (C7): Alternative names carry the shell's links: a DNS name to the DNS
//! inspector, an address to the IP inspector, a wildcard none.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::pages::views::{ViewContext, render_with};

fn blocks() -> Vec<Block> {
    let t = |s: &str| Value::Text(s.to_string());
    let leaf = vec![
        ("position".to_string(), t("leaf")),
        ("subject".into(), t("CN=example.com")),
        ("issuer".into(), t("CN=Example CA")),
        ("sans".into(), t("www.example.com")),
        ("sans".into(), t("*.example.com")),
        ("sans".into(), t("192.0.2.7")),
        ("fingerprint_sha256".into(), t("AA:BB")),
    ];
    vec![
        Block {
            id: BlockId::parse("tls.addresses").unwrap(),
            title: "Addresses".into(),
            shape: Shape::Table {
                key: "address".into(),
                columns: ["address", "port", "outcome", "message"]
                    .map(String::from)
                    .to_vec(),
                rows: vec![vec![
                    Value::Addr("192.0.2.1".parse().unwrap()),
                    Value::Int(443),
                    t("ok"),
                    t(""),
                ]],
            },
        },
        Block {
            id: BlockId::parse("tls.chain@192.0.2.1:443").unwrap(),
            title: "Certificate chain".into(),
            shape: Shape::Chain(vec![leaf]),
        },
    ]
}

#[test]
fn alternative_names_link_names_and_addresses_but_not_wildcards() {
    let siblings = blocks();
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    let html = render_with(&siblings[0], &ctx, &siblings).expect("port view");
    assert!(
        html.contains("href=\"/inspect/dns?q=www.example.com\""),
        "{html}"
    );
    assert!(html.contains("href=\"/inspect/ip?ip=192.0.2.7\""), "{html}");
    assert!(!html.contains("q=*.example.com"), "{html}");
    assert!(!html.contains("q=%2A.example.com"), "{html}");
    assert!(html.contains("*.example.com"), "{html}");
}
