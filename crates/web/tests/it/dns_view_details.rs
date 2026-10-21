//! v2-dns-ui Phase 3 (C1 markup, C4, C6, C7, C10, C11, C12, C14): the DNS part view's toolbar,
//! card order attributes, record details (TTL, servers, explain, SOA serial age) and the
//! stylesheet rules the toolbar toggles drive.
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render_part};

const GOOGLE: &str = "udp:8.8.8.8:53,name=Google 1";

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn pair(key: &str, value: Value) -> (String, Value) {
    (key.to_string(), value)
}

fn record(rtype: &str, ttl: i64, data: &str, extra: Vec<(String, Value)>) -> Node {
    let mut fields = vec![
        pair("name", text("example.com.")),
        pair("type", text(rtype)),
        pair("ttl", Value::Int(ttl)),
        pair("data", text(data)),
    ];
    fields.extend(extra);
    Node {
        fields,
        children: vec![],
    }
}

fn card(rtype: &str, records: Vec<Node>) -> Node {
    let server = Node {
        fields: vec![
            pair("server", text(GOOGLE)),
            pair("address", text("8.8.8.8:53")),
            pair("name", text("Google 1")),
            pair("transport", text("udp")),
            pair("outcome", text("answered")),
            pair("response_time", Value::Duration(10)),
        ],
        children: records,
    };
    Node {
        fields: vec![
            pair("record_type", text(rtype)),
            pair("agreement", text("single")),
        ],
        children: vec![server],
    }
}

fn html_of(cards: Vec<Node>) -> String {
    let block = Block {
        id: BlockId::parse("dns.answers").unwrap(),
        title: "Answers".into(),
        shape: Shape::Tree(Node {
            fields: vec![
                pair("mode", text("query")),
                pair("finished", Value::Bool(true)),
            ],
            children: cards,
        }),
    };
    let statuses = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    render_part(&[block], &ctx)
        .expect("dns.answers is claimed")
        .html
}

fn one(rtype: &str, rec: Node) -> String {
    html_of(vec![card(rtype, vec![rec])])
}

#[test]
fn c14_toolbar_is_hidden_with_five_toggles_and_two_actions() {
    let html = html_of(vec![card(
        "A",
        vec![record("A", 300, "192.0.2.10", vec![])],
    )]);
    assert!(
        html.contains(r#"<div class="dns-view__toolbar" role="toolbar" aria-label="View" hidden>"#),
        "{html}"
    );
    for toggle in [
        "hide-empty",
        "compact",
        "only-divergent",
        "sort-slowest",
        "explain",
    ] {
        let button = format!(r#"data-toggle="{toggle}" aria-pressed="false""#);
        assert!(html.contains(&button), "{toggle}\n{html}");
    }
    assert!(html.contains(r#"data-action="expand""#), "{html}");
    assert!(html.contains(r#"data-action="collapse""#), "{html}");
}

#[test]
fn c4_cards_carry_order_and_records_are_closed_details() {
    let html = html_of(vec![
        card("A", vec![record("A", 300, "192.0.2.10", vec![])]),
        card("AAAA", vec![record("AAAA", 300, "2001:db8::1", vec![])]),
    ]);
    let second = html.split(r#"data-type="AAAA""#).nth(1).expect("AAAA card");
    assert!(
        second
            .split('>')
            .next()
            .unwrap()
            .contains(r#"data-order="2""#),
        "{html}"
    );
    let first = html.split(r#"data-type="A""#).nth(1).unwrap();
    let first_tag = first.split('>').next().unwrap();
    assert!(first_tag.contains(r#"data-order="1""#), "{html}");
    assert!(first_tag.contains("data-rank="), "{html}");
    assert!(html.contains(r#"<details class="dns-record">"#), "{html}");
    assert!(
        !html.contains("<details class=\"dns-record\" open"),
        "{html}"
    );
}

#[test]
fn c6_c7_c10_stylesheet_rules_for_the_toggles() {
    let css =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/static/pages.css")).unwrap();
    let divergent = r#".dns-view[data-only-divergent] .dns-type:not([data-agreement="diverge"])"#;
    let at = css.find(divergent).expect("only-divergent selector");
    let body = &css[at..];
    let rule = &body[..body.find('}').unwrap()];
    assert!(rule.contains("display: none"), "{rule}");
    assert!(
        css.contains(r#".dns-view[data-hide-empty] .dns-group[data-outcome="no_records"]"#),
        "hide-empty selector"
    );
    assert!(
        css.contains(".dns-view[data-explain] .dns-record__explain"),
        "explain-on selector"
    );
    let hide = css.match_indices(".dns-record__explain").any(|(i, _)| {
        let rest = &css[i..];
        let brace = rest.find('{').unwrap_or(0);
        let head = &rest[..brace];
        !head.contains("data-explain")
            && !head.contains('}')
            && rest[brace..rest.find('}').unwrap_or(rest.len())].contains("display: none")
    });
    assert!(hide, "a rule hiding .dns-record__explain");
}

#[test]
fn c10_explain_sentence_renders_escaped_inside_the_record() {
    let sentence = "Mail for example.com goes to mx1.example.com. with priority 10";
    let html = one(
        "MX",
        record(
            "MX",
            300,
            "10 mx1.example.com.",
            vec![pair("explain", text(sentence))],
        ),
    );
    // Under the record, outside its closed details, so Explain shows it at once.
    let after = html.split("</details>").nth(1).expect("details");
    assert!(
        after.starts_with(&format!(r#"<p class="dns-record__explain">{sentence}</p>"#)),
        "{html}"
    );
    let escaped = one(
        "MX",
        record(
            "MX",
            300,
            "10 mx1.example.com.",
            vec![pair("explain", text("a <b> & c"))],
        ),
    );
    assert!(!escaped.contains("<b>"), "{escaped}");
    assert!(
        escaped.contains("a &#60;b&#62;") || escaped.contains("a &lt;b&gt;"),
        "{escaped}"
    );
}

#[test]
fn c11_ttl_in_human_units_and_the_answering_server() {
    let html = one("A", record("A", 3900, "192.0.2.10", vec![]));
    assert!(
        html.contains(r#"<dl class="dns-record__details">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<dd class="dns-record__ttl">1 h 5 min (3900 s)</dd>"#),
        "{html}"
    );
    let at = html
        .find(r#"<dd class="dns-record__server""#)
        .expect("server dd");
    let dd = &html[at..html[at..].find("</dd>").map(|e| at + e).unwrap()];
    assert!(
        dd.contains("<code>udp:8.8.8.8:53,name=Google 1</code>"),
        "{dd}"
    );
    assert!(dd.contains("udp"), "{dd}");
    assert!(dd.contains("10 ms"), "{dd}");

    for (ttl, want) in [(300, "5 min (300 s)"), (59, "59 s")] {
        let html = one("A", record("A", ttl, "192.0.2.10", vec![]));
        assert!(
            html.contains(&format!(r#"<dd class="dns-record__ttl">{want}</dd>"#)),
            "{ttl}\n{html}"
        );
    }
}

#[test]
fn c12_soa_serial_age_in_the_summary_line() {
    let data = "ns1.example.com. hostmaster.example.com. 2026101501 7200 3600 1209600 300";
    let aged = |days: i64| {
        one(
            "SOA",
            record(
                "SOA",
                300,
                data,
                vec![pair("soa_serial_age_days", Value::Int(days))],
            ),
        )
    };
    assert!(
        aged(4)
            .contains(r#"<span class="dns-record__serial">serial 2026101501 · 4 days old</span>"#),
        "{}",
        aged(4)
    );
    assert!(
        aged(1).contains("serial 2026101501 · 1 day old<"),
        "{}",
        aged(1)
    );
    let plain = one("SOA", record("SOA", 300, data, vec![]));
    assert!(!plain.contains("dns-record__serial"), "{plain}");
}
