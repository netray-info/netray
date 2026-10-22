//! v2-dns-ui Phase 6 reading: a not-measured answer draws no response-time bar beside the
//! answered ones, so a failure never reads as an answer's speed.
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

fn answer(server: &str, outcome: &str, extra: Vec<(String, Value)>) -> Node {
    let mut fields = vec![
        field("server", text(server)),
        field("transport", text("udp")),
        field("outcome", text(outcome)),
        field("response_time", Value::Duration(10)),
    ];
    fields.extend(extra);
    let records = if outcome == "answered" {
        vec![Node {
            fields: vec![
                field("name", text("partial.example.net")),
                field("type", text("A")),
                field("ttl", Value::Int(300)),
                field("data", text("192.0.2.10")),
            ],
            children: vec![],
        }]
    } else {
        vec![]
    };
    Node {
        fields,
        children: records,
    }
}

#[test]
fn a_not_measured_answer_draws_no_bar() {
    let card = Node {
        fields: vec![
            field("record_type", text("A")),
            field("agreement", text("single")),
        ],
        children: vec![
            answer("udp:192.0.2.1:53", "answered", vec![]),
            answer(
                "udp:192.0.2.2:53",
                "unmeasured",
                vec![field("response_code", text("SERVFAIL"))],
            ),
        ],
    };
    let block = Block {
        id: BlockId::parse("dns.answers").unwrap(),
        title: "Answers".into(),
        shape: Shape::Tree(Node {
            fields: vec![
                field("mode", text("query")),
                field("finished", Value::Bool(true)),
            ],
            children: vec![card],
        }),
    };
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    let html = render_part(&[block], &ctx).expect("claimed").html;
    assert_eq!(html.matches("dns-bar__fill").count(), 1, "{html}");
    let failed = html
        .split(r#"data-server="udp:192.0.2.2:53""#)
        .nth(1)
        .and_then(|rest| rest.split("</li>").next())
        .expect("failed server row");
    assert!(!failed.contains("dns-bar"), "{failed}");
    assert!(failed.contains("10 ms"), "{failed}");
}
