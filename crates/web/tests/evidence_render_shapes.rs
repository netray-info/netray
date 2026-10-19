//! V2 spec v2-inspectors, requirement 1 (C1, C3): `block_view` maps a model `Block` of each of
//! the six shapes to a view, and the rendered markup carries the shape, the id and every value
//! of the block in order.
use netray_model::evidence::{Block, BlockId, Node, Record, Shape, Value};
use netray_web::components::evidence_block;
use netray_web::evidence::block_view;

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

/// Asserts `needles` occur in `html`, each after the previous one.
fn assert_in_order(html: &str, needles: &[&str]) {
    let mut from = 0;
    for n in needles {
        let at = html[from..].find(n).unwrap_or_else(|| {
            panic!("`{n}` missing or out of order after byte {from} in: {html}")
        });
        from += at + n.len();
    }
}

#[test]
fn block_view_renders_every_shape_with_shape_id_and_values_in_order() {
    // (block, expected data-shape, expected id attribute, values in expected order)
    let cases: Vec<(Block, &str, &str, Vec<&str>)> = vec![
        (
            block(
                "dns.records",
                Shape::Records(vec![
                    Record {
                        owner: text("www.example.com"),
                        rtype: text("A"),
                        ttl: Value::Int(3600),
                        data: addr("192.0.2.10"),
                    },
                    Record {
                        owner: text("www.example.com"),
                        rtype: text("AAAA"),
                        ttl: Value::Int(7200),
                        data: addr("2001:db8::10"),
                    },
                ]),
            ),
            "records",
            "ev-dns.records",
            vec![
                "www.example.com",
                "A",
                "3600",
                "192.0.2.10",
                "www.example.com",
                "AAAA",
                "7200",
                "2001:db8::10",
            ],
        ),
        (
            block(
                "dns.resolvers@192.0.2.1",
                Shape::Table {
                    key: "resolver".into(),
                    columns: vec!["latency".into(), "resolver".into()],
                    rows: vec![
                        vec![Value::Int(23), text("ns1.example.com")],
                        vec![Value::Int(41), text("ns2.example.com")],
                    ],
                },
            ),
            "table",
            "ev-dns.resolvers@192.0.2.1",
            vec![
                "latency",
                "resolver",
                "23",
                "ns1.example.com",
                "41",
                "ns2.example.com",
            ],
        ),
        (
            block(
                "tls.chain@192.0.2.1:443",
                Shape::Chain(vec![
                    vec![
                        ("subject".into(), text("leaf.example.com")),
                        ("issuer".into(), text("Example CA")),
                    ],
                    vec![
                        ("subject".into(), text("Example CA")),
                        ("issuer".into(), text("Example Root")),
                    ],
                ]),
            ),
            "chain",
            "ev-tls.chain@192.0.2.1:443",
            vec![
                "leaf.example.com",
                "Example CA",
                "Example CA",
                "Example Root",
            ],
        ),
        (
            block(
                "dns.delegation",
                Shape::Tree(Node {
                    fields: vec![("name".into(), text("example.com"))],
                    children: vec![Node {
                        fields: vec![
                            ("name".into(), text("ns1.example.com")),
                            ("address".into(), addr("192.0.2.53")),
                        ],
                        children: vec![Node {
                            fields: vec![("name".into(), text("glue.example.com"))],
                            children: vec![],
                        }],
                    }],
                }),
            ),
            "tree",
            "ev-dns.delegation",
            vec![
                "data-depth=\"0\"",
                "name: example.com",
                "data-depth=\"1\"",
                "name: ns1.example.com, address: 192.0.2.53",
                "data-depth=\"2\"",
                "name: glue.example.com",
            ],
        ),
        (
            block(
                "tls.session",
                Shape::Fields(vec![
                    ("protocol".into(), text("TLSv1.3")),
                    ("san".into(), text("a.example.com")),
                    ("san".into(), text("b.example.com")),
                    ("cipher".into(), text("AES-256")),
                ]),
            ),
            "fields",
            "ev-tls.session",
            vec!["TLSv1.3", "a.example.com", "b.example.com", "AES-256"],
        ),
        (
            block(
                "http.headers",
                Shape::Text("HTTP/1.1 200 OK\nServer: example".into()),
            ),
            "text",
            "ev-http.headers",
            vec!["HTTP/1.1 200 OK\nServer: example"],
        ),
    ];
    for (b, shape, id, values) in cases {
        let view = block_view(&b);
        assert_eq!(view.id, id);
        assert_eq!(view.title, "Title");
        assert!(view.feeds.is_empty());
        let html = evidence_block(&view).unwrap();
        assert!(
            html.contains(&format!("data-shape=\"{shape}\"")),
            "no data-shape `{shape}` in: {html}"
        );
        assert!(
            html.contains(&format!("id=\"{id}\"")),
            "no id `{id}` in: {html}"
        );
        assert_in_order(&html, &values);
    }
}

#[test]
fn block_view_marks_only_the_key_column() {
    let b = block(
        "dns.nameservers",
        Shape::Table {
            key: "resolver".into(),
            columns: vec!["latency".into(), "resolver".into(), "status".into()],
            rows: vec![vec![Value::Int(23), text("ns1.example.com"), text("ok")]],
        },
    );
    let html = evidence_block(&block_view(&b)).unwrap();
    assert_eq!(html.matches("data-key=\"true\"").count(), 1, "in: {html}");
    let marked = html.find("data-key=\"true\"").unwrap();
    let key = html.find("resolver").unwrap();
    let next = html.find("status").unwrap();
    assert!(
        marked < key && key < next,
        "mark not on the resolver header: {html}"
    );
    assert!(
        html.find("latency").unwrap() < marked,
        "mark before latency: {html}"
    );
}
