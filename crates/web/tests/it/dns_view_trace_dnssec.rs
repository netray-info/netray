//! v2-dns-ui Phase 4 (C1 to C6, C8): the `dns.trace` and `dns.dnssec_chain` views.
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_web::evidence::display_value;
use netray_web::pages::views::{ViewContext, render};

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn node(fields: Vec<(&str, Value)>, children: Vec<Node>) -> Node {
    Node {
        fields: fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        children,
    }
}

fn block(id: &str, title: &str, root: Node) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: title.into(),
        shape: Shape::Tree(root),
    }
}

fn try_view(block: &Block) -> Option<String> {
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    render(block, &ctx)
}

fn view(block: &Block) -> String {
    try_view(block).expect("the block is claimed")
}

/// askama 0.16's HTML escaping.
fn escaped(text: &str) -> String {
    text.replace('&', "&#38;")
        .replace('<', "&#60;")
        .replace('>', "&#62;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

fn addr(s: &str) -> Value {
    Value::Addr(s.parse().unwrap())
}

fn server(fields: Vec<(&str, Value)>) -> Node {
    node(fields, vec![])
}

fn trace_root(hop0_groups: i64, extra: bool) -> Node {
    let mut hop0 = vec![
        ("level", Value::Int(0)),
        ("zone", text(".")),
        ("servers_queried", Value::Int(2)),
    ];
    if hop0_groups > 0 {
        hop0.push(("referral_groups", Value::Int(hop0_groups)));
    }
    if extra {
        hop0.push(("extra", text("x")));
    }
    node(
        vec![("hops", Value::Int(3))],
        vec![
            node(
                hop0,
                vec![
                    server(vec![
                        ("server_name", text("a.root-servers.net")),
                        ("address", addr("198.41.0.4")),
                        ("latency", Value::Duration(40)),
                        ("outcome", text("referral")),
                        ("referral", text("a.gtld-servers.net.")),
                    ]),
                    server(vec![
                        ("address", addr("199.9.14.201")),
                        ("outcome", text("error")),
                        ("error", text("connection timed out")),
                    ]),
                ],
            ),
            node(
                vec![
                    ("level", Value::Int(1)),
                    ("zone", text("com.")),
                    ("servers_queried", Value::Int(1)),
                    ("referral_groups", Value::Int(1)),
                ],
                vec![server(vec![
                    ("server_name", text("a.gtld-servers.net")),
                    ("address", addr("192.5.6.30")),
                    ("latency", Value::Duration(10)),
                    ("outcome", text("referral")),
                    ("referral", text("ns1.example.com.")),
                    ("referral", text("ns2.example.com.")),
                ])],
            ),
            node(
                vec![
                    ("level", Value::Int(2)),
                    ("zone", text("example.com.")),
                    ("servers_queried", Value::Int(1)),
                ],
                vec![server(vec![
                    ("server_name", text("ns1.example.com")),
                    ("address", addr("192.0.2.53")),
                    ("latency", Value::Duration(20)),
                    ("outcome", text("answer")),
                    ("answer", text("MX 10 mx1.example.com.")),
                ])],
            ),
        ],
    )
}

fn trace(groups: i64) -> Block {
    block("dns.trace", "Delegation trace", trace_root(groups, false))
}

fn finding(severity: &str, message: &str) -> Node {
    node(
        vec![("severity", text(severity)), ("message", text(message))],
        vec![],
    )
}

fn dnssec() -> Block {
    block(
        "dns.dnssec_chain",
        "DNSSEC chain",
        node(
            vec![("levels", Value::Int(3))],
            vec![
                node(
                    vec![
                        ("level", Value::Int(0)),
                        ("zone", text(".")),
                        ("latency", Value::Duration(5)),
                    ],
                    vec![finding("ok", "Root zone signed")],
                ),
                node(
                    vec![
                        ("level", Value::Int(1)),
                        ("zone", text("com.")),
                        ("latency", Value::Duration(12)),
                    ],
                    vec![finding("warning", "DS uses SHA-1 digest")],
                ),
                node(
                    vec![
                        ("level", Value::Int(2)),
                        ("zone", text("example.com.")),
                        ("latency", Value::Duration(30)),
                        ("dnskey", text("257 3 13 mdsswUyr3DPW132mOi8V9xESWE8jTo0d")),
                        ("dnskey", text("256 3 13 oJMRESz5E4gYzS/q6XDrvU1qMPYIjCWz")),
                        (
                            "ds",
                            text("2371 13 2 BE74359954660069D5C63D200C39F5603827D7DD"),
                        ),
                        (
                            "rrsig",
                            text("DNSKEY 13 2 3600 20261101000000 example.com."),
                        ),
                    ],
                    vec![finding("failed", "RRSIG expired")],
                ),
            ],
        ),
    )
}

#[test]
fn c1_c3_dnssec_chain_lists_levels_findings_and_records() {
    let html = view(&dnssec());
    for needle in [
        r#"id="ev-dns.dnssec_chain""#,
        "DNSSEC chain",
        "3 levels · 1 warn · 1 fail",
        r#"<li data-tone="pass">Root zone signed</li>"#,
        r#"<li data-tone="warn">DS uses SHA-1 digest</li>"#,
        r#"<li data-tone="fail">RRSIG expired</li>"#,
        r#"<details class="dns-level__records">"#,
        "DNSKEY",
        "DS",
        "RRSIG",
    ] {
        assert!(html.contains(needle), "{needle}: {html}");
    }
    let root = html.find(r#"data-level="0""#).expect("root level");
    let com = html.find(r#"data-level="1""#).expect("com level");
    let leaf = html.find(r#"data-level="2""#).expect("leaf level");
    assert!(root < com && com < leaf, "{html}");
    assert_eq!(html.matches("dns-level__records").count(), 1, "{html}");
}

#[test]
fn c4_a_level_with_one_warning_carries_a_warn_badge() {
    let html = view(&dnssec());
    assert!(
        html.contains(r#"<span class="dns-level__badge" data-tone="warn">1 warn · 0 fail</span>"#),
        "{html}"
    );
}

#[test]
fn c2_c5_trace_lists_hops_servers_bars_referrals_and_answer() {
    let html = view(&trace(0));
    for needle in [
        r#"id="ev-dns.trace""#,
        "Delegation trace",
        "3 hops",
        r#"data-level="0""#,
        r#"data-level="1""#,
        r#"data-level="2""#,
        "a.root-servers.net",
        "198.41.0.4",
        "a.gtld-servers.net",
        "192.5.6.30",
        "ns1.example.com",
        "192.0.2.53",
        r#"<span class="dns-bar__fill" style="width: 100%">"#,
        r#"<span class="dns-bar__fill" style="width: 25%">"#,
        r#"<span class="dns-bar__fill" style="width: 50%">"#,
        "Referral: ns1.example.com., ns2.example.com.",
        r#"<code class="dns-hop__answer">MX 10 mx1.example.com.</code>"#,
        "connection timed out",
    ] {
        assert!(html.contains(needle), "{needle}: {html}");
    }
    assert_eq!(html.matches("dns-hop__error").count(), 1, "{html}");
    assert_eq!(html.matches("dns-bar__fill").count(), 3, "{html}");
}

#[test]
fn c6_two_referral_groups_mark_a_disagreeing_hop() {
    let marker = r#"<span class="dns-hop__disagree" data-tone="warn">servers disagree</span>"#;
    let two = view(&trace(2));
    assert_eq!(two.matches(marker).count(), 1, "{two}");
    let one = view(&trace(1));
    assert!(!one.contains("dns-hop__disagree"), "{one}");
}

#[test]
fn an_unknown_key_leaves_the_trace_unclaimed() {
    let b = block("dns.trace", "Delegation trace", trace_root(0, true));
    assert!(try_view(&b).is_none());
}

#[test]
fn c8_nothing_lost_every_stored_value_is_shown() {
    // Shown in other words, so exempt from the display-text check: `outcome` (data-outcome),
    // `severity` (data-tone), `referral_groups` (disagree marker), `hops`, `levels` and
    // `servers_queried` (meta line).
    const EXEMPT: [&str; 6] = [
        "outcome",
        "severity",
        "referral_groups",
        "hops",
        "levels",
        "servers_queried",
    ];
    fn walk(n: &Node, out: &mut Vec<String>) {
        for (key, value) in &n.fields {
            if !EXEMPT.contains(&key.as_str()) {
                out.push(escaped(&display_value(value)));
            }
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    for b in [trace(2), dnssec()] {
        let Shape::Tree(root) = &b.shape else {
            unreachable!()
        };
        let mut needles = vec![];
        walk(root, &mut needles);
        let html = view(&b);
        for needle in needles {
            assert!(html.contains(&needle), "{}: {needle}: {html}", b.id);
        }
    }
}
