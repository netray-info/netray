//! v2-dns-ui Phase 3 review: Compact never hides a failure group or empties a filtered card,
//! Only divergent keeps not-measured cards, the record details' server rows take part in the
//! server filter, and a serial dated after today reads no negative age.
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

fn css() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/static/pages.css")).unwrap()
}

fn render(cards: Vec<Node>) -> String {
    let block = Block {
        id: BlockId::parse("dns.answers").unwrap(),
        title: "Answers".into(),
        shape: Shape::Tree(node(
            vec![
                field("mode", text("query")),
                field("finished", Value::Bool(true)),
            ],
            cards,
        )),
    };
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    render_part(&[block], &ctx).expect("claimed").html
}

fn answer(server: &str, outcome: &str, extra: Vec<(String, Value)>, records: Vec<Node>) -> Node {
    let mut fields = vec![
        field("server", text(server)),
        field("transport", text("udp")),
        field("outcome", text(outcome)),
    ];
    fields.extend(extra);
    node(fields, records)
}

fn record(rtype: &str, data: &str, extra: Vec<(String, Value)>) -> Node {
    let mut fields = vec![
        field("name", text("example.com")),
        field("type", text(rtype)),
        field("ttl", Value::Int(300)),
        field("data", text(data)),
    ];
    fields.extend(extra);
    node(fields, vec![])
}

fn card(rtype: &str, agreement: &str, answers: Vec<Node>) -> Node {
    node(
        vec![
            field("record_type", text(rtype)),
            field("agreement", text(agreement)),
        ],
        answers,
    )
}

#[test]
fn compact_keeps_the_lead_group_and_every_failure() {
    let html = render(vec![card(
        "A",
        "single",
        vec![
            answer(
                "udp:192.0.2.1:53",
                "answered",
                vec![],
                vec![record("A", "192.0.2.10", vec![])],
            ),
            answer(
                "udp:192.0.2.2:53",
                "unmeasured",
                vec![field("response_code", text("SERVFAIL"))],
                vec![],
            ),
        ],
    )]);
    assert!(
        html.contains(r#"<div class="dns-group" data-outcome="answered" data-lead>"#),
        "{html}"
    );
    let css = css();
    assert!(
        !css.contains(".dns-view[data-compact] .dns-group + .dns-group"),
        "{css}"
    );
    assert!(
        css.contains(r#".dns-view[data-compact]:not([data-server]) .dns-group:not([data-lead]):not([data-outcome="unmeasured"])"#),
        "{css}"
    );
    assert!(
        css.contains(".dns-view[data-compact]:not([data-server]) .dns-record__details"),
        "{css}"
    );
}

#[test]
fn only_divergent_keeps_not_measured_cards() {
    assert!(css().contains(
        r#".dns-view[data-only-divergent] .dns-type:not([data-agreement="diverge"]):not([data-agreement="unmeasured"])"#
    ));
}

#[test]
fn record_detail_servers_carry_their_server() {
    let html = render(vec![card(
        "A",
        "single",
        vec![answer(
            "udp:192.0.2.1:53",
            "answered",
            vec![],
            vec![record("A", "192.0.2.10", vec![])],
        )],
    )]);
    assert!(
        html.contains(r#"<dd class="dns-record__server" data-server="udp:192.0.2.1:53">"#),
        "{html}"
    );
    let island = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/islands/src/dnsview.ts"
    ))
    .unwrap();
    assert!(island.contains(".dns-record__server"), "{island}");
}

#[test]
fn a_serial_dated_after_today_has_no_negative_age() {
    let soa = "ns1.example.com. hostmaster.example.com. 2026102201 7200 3600 1209600 300";
    let html = render(vec![card(
        "SOA",
        "single",
        vec![answer(
            "udp:192.0.2.1:53",
            "answered",
            vec![],
            vec![record(
                "SOA",
                soa,
                vec![field("soa_serial_age_days", Value::Int(-1))],
            )],
        )],
    )]);
    assert!(
        html.contains("serial 2026102201 · dated in the future"),
        "{html}"
    );
    assert!(!html.contains("-1 days"), "{html}");
}
