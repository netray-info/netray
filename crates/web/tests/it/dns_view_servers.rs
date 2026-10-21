//! v2-dns-ui Phase 2 (C1, C2 markup, C4, C5, C6): the DNS part view's Servers panel, from
//! hand-built `dns.answers` and `dns.servers` blocks.
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render_part};

const GOOGLE: &str = "udp:192.0.2.1:53,name=Google 1";
const CLOUD: &str = "udp:192.0.2.2:53,name=Cloudflare 1";
const AUTH: &str = "udp:192.0.2.3:53";

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn pair(key: &str, value: Value) -> (String, Value) {
    (key.to_string(), value)
}

fn record(data: &str) -> Node {
    Node {
        fields: vec![
            pair("name", text("example.com.")),
            pair("type", text("A")),
            pair("ttl", Value::Int(300)),
            pair("data", text(data)),
        ],
        children: vec![],
    }
}

/// One server answer: `outcome` plus its records, optional source and reason.
fn answer(
    server: &str,
    outcome: &str,
    records: &[&str],
    source: Option<&str>,
    error: Option<&str>,
) -> Node {
    let address = server.split(',').next().unwrap().trim_start_matches("udp:");
    let mut fields = vec![
        pair("server", text(server)),
        pair("address", text(address)),
        pair("transport", text("udp")),
        pair("outcome", text(outcome)),
    ];
    if let Some(name) = server.split("name=").nth(1) {
        fields.push(pair("name", text(name)));
    }
    if let Some(s) = source {
        fields.push(pair("source", text(s)));
    }
    if let Some(e) = error {
        fields.push(pair("error", text(e)));
    }
    if outcome == "answered" {
        fields.push(pair("response_time", Value::Duration(10)));
    }
    Node {
        fields,
        children: records.iter().map(|d| record(d)).collect(),
    }
}

fn type_node(record_type: &str, agreement: &str, servers: Vec<Node>) -> Node {
    Node {
        fields: vec![
            pair("record_type", text(record_type)),
            pair("agreement", text(agreement)),
        ],
        children: servers,
    }
}

fn answers(types: Vec<Node>) -> Block {
    Block {
        id: BlockId::parse("dns.answers").unwrap(),
        title: "Answers".into(),
        shape: Shape::Tree(Node {
            fields: vec![
                pair("mode", text("query")),
                pair("finished", Value::Bool(true)),
                pair("record_types", Value::Int(types.len() as i64)),
                pair("lookups", Value::Int(4)),
                pair("servers", Value::Int(3)),
                pair("agree", Value::Int(0)),
                pair("diverge", Value::Int(1)),
                pair("single", Value::Int(0)),
                pair("unmeasured", Value::Int(0)),
            ],
            children: types,
        }),
    }
}

struct Summary<'a> {
    server: &'a str,
    source: Option<&'a str>,
    lookups: i64,
    measured: i64,
    median: Option<u64>,
}

fn servers(rows: &[Summary]) -> Block {
    let children = rows
        .iter()
        .map(|r| {
            let mut fields = vec![pair("server", text(r.server))];
            if let Some(name) = r.server.split("name=").nth(1) {
                fields.push(pair("name", text(name)));
            }
            fields.push(pair("transport", text("udp")));
            if let Some(s) = r.source {
                fields.push(pair("source", text(s)));
            }
            fields.push(pair("lookups", Value::Int(r.lookups)));
            fields.push(pair("measured", Value::Int(r.measured)));
            fields.push(pair("unmeasured", Value::Int(r.lookups - r.measured)));
            if let Some(m) = r.median {
                fields.push(pair("median_response_time", Value::Duration(m)));
            }
            Node {
                fields,
                children: vec![],
            }
        })
        .collect();
    Block {
        id: BlockId::parse("dns.servers").unwrap(),
        title: "Servers".into(),
        shape: Shape::Tree(Node {
            fields: vec![pair("servers", Value::Int(rows.len() as i64))],
            children,
        }),
    }
}

fn html_of(blocks: Vec<Block>) -> String {
    let statuses = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    render_part(&blocks, &ctx)
        .expect("dns.answers is claimed by the DNS part view")
        .html
}

/// The `dns-server` card of `server`.
fn card<'a>(html: &'a str, server: &str) -> &'a str {
    let open = format!(r#"<article class="dns-server" data-server="{server}""#);
    let start = html
        .find(&open)
        .unwrap_or_else(|| panic!("no card for {server}: {html}"));
    let len = html[start..].find("</article>").expect("card closes");
    &html[start..start + len]
}

/// The `dns-server__type` item of `record_type` inside a card.
fn type_item<'a>(card: &'a str, record_type: &str) -> &'a str {
    let open = format!(r#"<li class="dns-server__type" data-type="{record_type}""#);
    let start = card
        .find(&open)
        .unwrap_or_else(|| panic!("no {record_type} item: {card}"));
    let len = card[start..].find("</li>").expect("item closes");
    &card[start..start + len]
}

/// A: Google and Cloudflare diverge; MX: Google answers, Cloudflare has no records.
fn fixture() -> Vec<Block> {
    vec![
        answers(vec![
            type_node(
                "A",
                "diverge",
                vec![
                    answer(GOOGLE, "answered", &["192.0.2.10"], None, None),
                    answer(CLOUD, "answered", &["192.0.2.20"], Some("anycast"), None),
                ],
            ),
            type_node(
                "MX",
                "single",
                vec![
                    answer(GOOGLE, "answered", &["10 mail.example.com."], None, None),
                    answer(CLOUD, "no_records", &[], Some("anycast"), None),
                ],
            ),
        ]),
        servers(&[
            Summary {
                server: GOOGLE,
                source: None,
                lookups: 2,
                measured: 2,
                median: Some(10),
            },
            Summary {
                server: CLOUD,
                source: Some("anycast"),
                lookups: 2,
                measured: 2,
                median: Some(10),
            },
        ]),
    ]
}

#[test]
fn c1_one_server_card_per_summary_row_in_order_with_its_facts_and_types() {
    let html = html_of(fixture());

    assert_eq!(
        html.matches(r#"<article class="dns-server""#).count(),
        2,
        "{html}"
    );
    assert!(
        html.find(GOOGLE).unwrap() < html.find(CLOUD).unwrap(),
        "{html}"
    );
    let g = card(&html, GOOGLE);
    assert!(
        g.contains(r#"<h4 class="dns-server__name">Google 1</h4>"#),
        "{g}"
    );
    assert!(
        g.contains(r#"<span class="dns-server__transport">udp</span>"#),
        "{g}"
    );
    assert!(!g.contains("dns-server__source"), "{g}");
    assert!(
        g.contains(r#"<span class="dns-server__measured">2 / 2 measured</span>"#),
        "{g}"
    );
    assert!(
        g.contains(r#"<span class="dns-server__median">median 10 ms</span>"#),
        "{g}"
    );
    assert_eq!(g.matches(r#"class="dns-server__type""#).count(), 2, "{g}");
    let a = type_item(g, "A");
    assert!(
        a.contains(r#"<span class="dns-server__type-name">A</span>"#),
        "{a}"
    );
    assert!(
        a.contains(r#"<code class="dns-server__record">"#) && a.contains("192.0.2.10"),
        "{a}"
    );
    assert!(!a.contains("192.0.2.20"), "{a}");
    assert!(type_item(g, "MX").contains("10 mail.example.com."), "{g}");

    let c = card(&html, CLOUD);
    assert!(
        c.contains(r#"<span class="dns-server__source">anycast</span>"#),
        "{c}"
    );
    assert!(c.contains("192.0.2.20"), "{c}");
    let mx = type_item(c, "MX");
    assert!(
        mx.contains(r#"<span class="dns-server__outcome">no records</span>"#),
        "{mx}"
    );
    assert!(!mx.contains("dns-server__record"), "{mx}");
}

#[test]
fn c4_negative_outcomes_read_as_words_in_the_servers_card() {
    let blocks = vec![
        answers(vec![type_node(
            "A",
            "single",
            vec![answer(GOOGLE, "nxdomain", &[], None, None)],
        )]),
        servers(&[Summary {
            server: GOOGLE,
            source: None,
            lookups: 1,
            measured: 1,
            median: None,
        }]),
    ];
    let html = html_of(blocks);

    let item = type_item(card(&html, GOOGLE), "A");
    assert!(
        item.contains(r#"<span class="dns-server__outcome">NXDOMAIN</span>"#),
        "{item}"
    );
}

#[test]
fn c5_an_unmeasured_server_has_no_median_and_reads_not_measured_with_the_reason() {
    let blocks = vec![
        answers(vec![
            type_node(
                "A",
                "unmeasured",
                vec![answer(AUTH, "unmeasured", &[], None, Some("timeout"))],
            ),
            type_node(
                "MX",
                "unmeasured",
                vec![answer(AUTH, "unmeasured", &[], None, Some("timeout"))],
            ),
        ]),
        servers(&[Summary {
            server: AUTH,
            source: None,
            lookups: 2,
            measured: 0,
            median: None,
        }]),
    ];
    let html = html_of(blocks);

    let c = card(&html, AUTH);
    assert!(!c.contains("dns-server__median"), "{c}");
    assert!(
        c.contains(r#"<span class="dns-server__measured">0 / 2 measured</span>"#),
        "{c}"
    );
    for t in ["A", "MX"] {
        let item = type_item(c, t);
        assert!(
            item.contains(r#"<span class="dns-server__outcome">not measured: timeout</span>"#),
            "{item}"
        );
    }
}

#[test]
fn c6_a_diverging_type_is_marked_in_every_server_card() {
    let html = html_of(fixture());

    for s in [GOOGLE, CLOUD] {
        let a = type_item(card(&html, s), "A");
        assert!(a.contains(r#"data-agreement="diverge""#), "{a}");
        assert!(
            a.contains(r#"<span class="dns-server__diverge" data-tone="warn">diverge</span>"#),
            "{a}"
        );
        let mx = type_item(card(&html, s), "MX");
        assert!(mx.contains(r#"data-agreement="single""#), "{mx}");
        assert!(!mx.contains("dns-server__diverge"), "{mx}");
    }
}

#[test]
fn c2_markup_root_island_and_hidden_filter_buttons() {
    let html = html_of(fixture());

    let root = &html[html.find(r#"<section class="dns-view""#).expect("root")..];
    let root_tag = &root[..root.find('>').unwrap()];
    assert!(root_tag.contains(r#"data-island="dnsview""#), "{root_tag}");
    assert_eq!(
        html.matches(
            r#"<button type="button" class="dns-view__all" data-server-filter="" hidden>"#
        )
        .count(),
        1,
        "{html}"
    );
    for s in [GOOGLE, CLOUD] {
        let open = format!(
            r#"<button type="button" class="dns-server__filter" data-server-filter="{s}" hidden>"#
        );
        assert!(card(&html, s).contains(&open), "{s}: {html}");
    }
}
