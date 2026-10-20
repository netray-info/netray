//! v2-dns-ui Phase 1 review: the DNS part view keeps the shell's links on record data, names a
//! referral by its error rather than its NOERROR code, claims a "not measured" block only when its
//! reason is shown, and shows each record's own TTL.
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::links::decorate;
use netray_web::pages::views::{PartView, ViewContext, render_part};

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn field(key: &str, value: Value) -> (String, Value) {
    (key.to_string(), value)
}

fn node(fields: Vec<(String, Value)>, children: Vec<Node>) -> Node {
    Node { fields, children }
}

fn server(address: &str, outcome: &str, extra: Vec<(String, Value)>, records: Vec<Node>) -> Node {
    let mut fields = vec![
        field("server", text(&format!("udp:{address}:53"))),
        field("address", text(&format!("{address}:53"))),
        field("transport", text("udp")),
        field("outcome", text(outcome)),
    ];
    fields.extend(extra);
    node(fields, records)
}

fn record(rtype: &str, ttl: i64, data: &str) -> Node {
    node(
        vec![
            field("name", text("www.example.com")),
            field("type", text(rtype)),
            field("ttl", Value::Int(ttl)),
            field("data", text(data)),
        ],
        vec![],
    )
}

fn type_card(record_type: &str, agreement: &str, servers: Vec<Node>) -> Node {
    node(
        vec![
            field("record_type", text(record_type)),
            field("agreement", text(agreement)),
        ],
        servers,
    )
}

fn answers(cards: Vec<Node>) -> Block {
    Block {
        id: BlockId::parse("dns.answers").unwrap(),
        title: "Answers".into(),
        shape: Shape::Tree(node(
            vec![
                field("mode", text("query")),
                field("finished", Value::Bool(true)),
            ],
            cards,
        )),
    }
}

fn not_measured(id: &str, reason: &str) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: "A".into(),
        shape: Shape::Fields(vec![
            field("outcome", text("not measured")),
            field("reason", text(reason)),
        ]),
    }
}

fn render(blocks: &[Block]) -> PartView {
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    render_part(blocks, &ctx).expect("dns.answers is claimed")
}

#[test]
fn record_data_keeps_the_shells_links() {
    let card = type_card(
        "A",
        "single",
        vec![server(
            "192.0.2.53",
            "answered",
            vec![],
            vec![
                record("CNAME", 300, "edge.example.com."),
                record("A", 300, "192.0.2.10"),
            ],
        )],
    );
    let html = render(&[answers(vec![card])]).html;
    assert!(
        html.contains(r#"href="/inspect/ip?ip=192.0.2.10"><code class="dns-record__data">192.0.2.10</code></a>"#),
        "{html}"
    );
    assert!(html.contains(r#"href="/inspect/dns?q="#), "{html}");
}

#[test]
fn a_referral_reads_its_error_and_its_block_is_claimed_only_with_that_reason() {
    let referral = vec![
        field("response_code", text("NOERROR")),
        field("error", text("referral, no answer")),
    ];
    let card = type_card(
        "A",
        "unmeasured",
        vec![server("192.0.2.53", "unmeasured", referral, vec![])],
    );
    let shown = render(&[
        answers(vec![card.clone()]),
        not_measured("dns.a", "referral, no answer"),
    ]);
    assert!(
        shown.html.contains("not measured: referral, no answer"),
        "{}",
        shown.html
    );
    assert!(
        !shown.html.contains("not measured: NOERROR"),
        "{}",
        shown.html
    );
    assert!(shown.claimed.iter().any(|id| id.to_string() == "dns.a"));

    let other = render(&[answers(vec![card]), not_measured("dns.a", "timed out")]);
    assert!(!other.claimed.iter().any(|id| id.to_string() == "dns.a"));
}

#[test]
fn each_record_shows_its_own_ttl() {
    let records = || {
        vec![
            record("CNAME", 3600, "cdn.example.com."),
            record("CNAME", 600, "edge.example.com."),
            record("A", 60, "192.0.2.10"),
        ]
    };
    let card = type_card(
        "A",
        "agree",
        vec![
            server("192.0.2.53", "answered", vec![], records()),
            server("198.51.100.53", "answered", vec![], records()),
        ],
    );
    let html = render(&[answers(vec![card])]).html;
    assert!(html.contains("TTL 600 s"), "{html}");
}
