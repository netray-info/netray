//! v2-dns-ui Phase 2 review: an authcompare server that prints alike as recursive and
//! authoritative keeps two cards with their own answers and filter keys, and a job that never
//! finished carries no pass mark on its tabs.
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render_part};

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn field(key: &str, value: Value) -> (String, Value) {
    (key.to_string(), value)
}

fn node(fields: Vec<(String, Value)>, children: Vec<Node>) -> Node {
    Node { fields, children }
}

const SERVER: &str = "udp:192.0.2.53:53";

fn answer(source: &str, outcome: &str, extra: Vec<(String, Value)>, data: Option<&str>) -> Node {
    let mut fields = vec![
        field("server", text(SERVER)),
        field("address", text("192.0.2.53:53")),
        field("transport", text("udp")),
        field("source", text(source)),
        field("outcome", text(outcome)),
    ];
    fields.extend(extra);
    let records = data
        .map(|d| {
            vec![node(
                vec![
                    field("name", text("example.com")),
                    field("type", text("A")),
                    field("ttl", Value::Int(300)),
                    field("data", text(d)),
                ],
                vec![],
            )]
        })
        .unwrap_or_default();
    node(fields, records)
}

fn summary(source: &str, measured: i64) -> Node {
    node(
        vec![
            field("server", text(SERVER)),
            field("transport", text("udp")),
            field("source", text(source)),
            field("lookups", Value::Int(1)),
            field("measured", Value::Int(measured)),
            field("unmeasured", Value::Int(1 - measured)),
        ],
        vec![],
    )
}

fn blocks(finished: bool, cards: Vec<Node>, servers: Vec<Node>) -> Vec<Block> {
    vec![
        Block {
            id: BlockId::parse("dns.answers").unwrap(),
            title: "Answers".into(),
            shape: Shape::Tree(node(
                vec![
                    field("mode", text("authcompare")),
                    field("finished", Value::Bool(finished)),
                ],
                cards,
            )),
        },
        Block {
            id: BlockId::parse("dns.servers").unwrap(),
            title: "Servers".into(),
            shape: Shape::Tree(node(vec![], servers)),
        },
    ]
}

fn render(blocks: &[Block]) -> String {
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    render_part(blocks, &ctx).expect("claimed").html
}

/// The server card whose filter key is `key`.
fn card<'a>(html: &'a str, key: &str) -> &'a str {
    let open =
        format!(r#"<article class="dns-server" data-server="{SERVER}" data-filter-key="{key}">"#);
    let start = html
        .find(&open)
        .unwrap_or_else(|| panic!("no card {key}: {html}"));
    let end = html[start..].find("</article>").unwrap() + start;
    &html[start..end]
}

#[test]
fn recursive_and_authoritative_cards_keep_their_own_answers() {
    let card_node = node(
        vec![
            field("record_type", text("A")),
            field("agreement", text("single")),
        ],
        vec![
            answer("authoritative", "answered", vec![], Some("192.0.2.10")),
            answer(
                "recursive",
                "unmeasured",
                vec![field("error", text("timed out"))],
                None,
            ),
        ],
    );
    let html = render(&blocks(
        true,
        vec![card_node],
        vec![summary("authoritative", 1), summary("recursive", 0)],
    ));
    let auth = card(&html, &format!("authoritative {SERVER}"));
    let rec = card(&html, &format!("recursive {SERVER}"));
    assert!(auth.contains("192.0.2.10"), "{auth}");
    assert!(rec.contains("not measured: timed out"), "{rec}");
    assert!(!rec.contains("192.0.2.10"), "{rec}");
    assert!(
        rec.contains(&format!(r#"data-server-filter="recursive {SERVER}""#)),
        "{rec}"
    );
    assert!(
        rec.contains("192.0.2.53:53"),
        "label falls back to the address: {rec}"
    );
}

#[test]
fn an_unfinished_job_has_no_pass_mark() {
    let card_node = node(
        vec![
            field("record_type", text("A")),
            field("agreement", text("single")),
        ],
        vec![answer(
            "authoritative",
            "answered",
            vec![],
            Some("192.0.2.10"),
        )],
    );
    let html = render(&blocks(
        false,
        vec![card_node],
        vec![summary("authoritative", 1)],
    ));
    assert!(!html.contains(r#"data-tone="pass">✓</span>"#), "{html}");
}
