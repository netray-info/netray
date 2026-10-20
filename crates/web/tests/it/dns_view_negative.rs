//! v2-dns-ui Phase 1 (R6, C6, C12, C13): negative answers read in the DNS part view's words and
//! sit beside the answered groups of the same card (Rule M).
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

/// One server answer; `extra` carries outcome-specific fields, `records` the answer's records.
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

fn record(data: &str) -> Node {
    node(
        vec![
            field("name", text("example.com")),
            field("type", text("A")),
            field("ttl", Value::Int(300)),
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
    let fields = vec![
        field("mode", text("query")),
        field("finished", Value::Bool(true)),
        field("record_types", Value::Int(cards.len() as i64)),
        field("lookups", Value::Int(2)),
        field("servers", Value::Int(2)),
    ];
    Block {
        id: BlockId::parse("dns.answers").unwrap(),
        title: "Answers".into(),
        shape: Shape::Tree(node(fields, cards)),
    }
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
    render_part(blocks, &ctx)
        .expect("dns.answers is claimed by the DNS part view")
        .html
}

/// The trimmed text of every element opened by `open`, in document order.
fn texts_after(html: &str, open: &str) -> Vec<String> {
    html.split(open)
        .skip(1)
        .map(|rest| {
            let end = rest.find("</").unwrap_or(rest.len());
            rest[..end].trim().to_string()
        })
        .collect()
}

fn outcomes(html: &str) -> Vec<String> {
    texts_after(html, "<p class=\"dns-group__outcome\">")
}

fn badges(html: &str) -> Vec<String> {
    texts_after(html, "<span class=\"dns-type__badge\" data-tone=\"muted\">")
}

#[test]
fn negative_outcomes_read_in_words_beside_answered_groups() {
    let servers = vec![
        server("192.0.2.1", "answered", vec![], vec![record("192.0.2.10")]),
        server(
            "192.0.2.2",
            "no_records",
            vec![field("response_code", text("NOERROR"))],
            vec![],
        ),
        server(
            "192.0.2.3",
            "nxdomain",
            vec![field("response_code", text("NXDOMAIN"))],
            vec![],
        ),
        server(
            "192.0.2.4",
            "unmeasured",
            vec![field("response_code", text("SERVFAIL"))],
            vec![],
        ),
        server(
            "192.0.2.5",
            "unmeasured",
            vec![field("error", text("timeout"))],
            vec![],
        ),
    ];
    let html = render(&[answers(vec![type_card("A", "diverge", servers)])]);

    let got = outcomes(&html);
    for want in [
        "no records",
        "NXDOMAIN",
        "not measured: SERVFAIL",
        "not measured: timeout",
    ] {
        assert!(
            got.contains(&want.to_string()),
            "{want:?} missing in {got:?}"
        );
    }
    assert!(html.contains("192.0.2.10"), "answered group stays: {html}");
    assert!(html.contains("data-outcome=\"answered\""), "{html}");
}

#[test]
fn single_answer_with_one_unmeasured_server() {
    let servers = vec![
        server("192.0.2.1", "answered", vec![], vec![record("192.0.2.10")]),
        server(
            "192.0.2.2",
            "unmeasured",
            vec![field("response_code", text("SERVFAIL"))],
            vec![],
        ),
    ];
    let html = render(&[answers(vec![type_card("A", "single", servers)])]);

    assert_eq!(badges(&html), vec!["single answer".to_string()], "{html}");
    assert!(html.contains("192.0.2.10"), "{html}");
    assert_eq!(
        outcomes(&html),
        vec!["not measured: SERVFAIL".to_string()],
        "{html}"
    );
}

#[test]
fn every_server_no_records_reads_no_records() {
    let servers = vec![
        server(
            "192.0.2.1",
            "no_records",
            vec![field("response_code", text("NOERROR"))],
            vec![],
        ),
        server(
            "192.0.2.2",
            "no_records",
            vec![field("response_code", text("NOERROR"))],
            vec![],
        ),
    ];
    let html = render(&[answers(vec![type_card("AAAA", "agree", servers)])]);

    assert!(
        outcomes(&html).contains(&"no records".to_string()),
        "{html}"
    );
}
