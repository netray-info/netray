//! v2-dns-ui Phase 1 (C7, C14 render half): a record shows its zone-file data in
//! `dns-record__data` and its human text in `dns-record__human` only where that differs.
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render_part};

/// askama 0.16's HTML escaping.
fn escaped(text: &str) -> String {
    text.replace('&', "&#38;")
        .replace('<', "&#60;")
        .replace('>', "&#62;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

fn t(s: &str) -> Value {
    Value::Text(s.into())
}

fn node(fields: Vec<(&str, Value)>, children: Vec<Node>) -> Node {
    Node {
        fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        children,
    }
}

fn answers(record_type: &str, data: &str, human: Option<&str>) -> Block {
    let mut fields = vec![
        ("name", t("example.com.")),
        ("type", t(record_type)),
        ("ttl", Value::Int(300)),
        ("data", t(data)),
    ];
    if let Some(h) = human {
        fields.push(("human", t(h)));
    }
    let record = node(fields, vec![]);
    let server = node(
        vec![
            ("server", t("udp:192.0.2.53:53,name=Test 1")),
            ("address", t("192.0.2.53:53")),
            ("name", t("Test 1")),
            ("transport", t("udp")),
            ("outcome", t("answered")),
            ("response_time", Value::Duration(10)),
        ],
        vec![record],
    );
    let ty = node(
        vec![("record_type", t(record_type)), ("agreement", t("single"))],
        vec![server],
    );
    let root = node(
        vec![
            ("mode", t("query")),
            ("finished", Value::Bool(true)),
            ("record_types", Value::Int(1)),
            ("lookups", Value::Int(1)),
            ("servers", Value::Int(1)),
            ("agree", Value::Int(0)),
            ("diverge", Value::Int(0)),
            ("single", Value::Int(1)),
            ("unmeasured", Value::Int(0)),
        ],
        vec![ty],
    );
    Block {
        id: BlockId::parse("dns.answers").unwrap(),
        title: "Answers".into(),
        shape: Shape::Tree(root),
    }
}

fn html(block: Block) -> String {
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

#[test]
fn r7_human_shows_only_where_it_differs_from_the_data() {
    // (type, data, human, expect a human span)
    let rows: [(&str, &str, Option<&str>, bool); 3] = [
        ("A", "192.0.2.10", Some("IPv4 192.0.2.10"), true),
        ("A", "192.0.2.10", Some("192.0.2.10"), false),
        ("A", "192.0.2.10", None, false),
    ];
    for (ty, data, human, shown) in rows {
        let out = html(answers(ty, data, human));
        assert!(
            out.contains("<code class=\"dns-record__data\">192.0.2.10</code>"),
            "data in code: {human:?}\n{out}"
        );
        assert_eq!(
            out.contains("dns-record__human"),
            shown,
            "human {human:?}\n{out}"
        );
        if let (true, Some(h)) = (shown, human) {
            assert!(out.contains(&escaped(h)), "human text {h:?}\n{out}");
        }
    }
}

#[test]
fn c14_txt_human_text_shows_escaped_in_the_human_span() {
    let data = "\"v=spf1 include:_spf.example.com -all\"";
    let human = "SPF version 1; include <_spf.example.com> & fail all";
    let out = html(answers("TXT", data, Some(human)));
    let start = out
        .find("<span class=\"dns-record__human\">")
        .unwrap_or_else(|| panic!("human span\n{out}"));
    let tail = &out[start..];
    let span = &tail[..tail.find("</span>").expect("span closes")];
    assert!(span.contains(&escaped(human)), "span: {span}");
    assert!(out.contains("dns-record__data"), "data code\n{out}");
}
