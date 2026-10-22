//! v2-dns-ui Phase 2, requirement 10: the tab badges inside the tab links (C3, C8, C9).
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render_part};

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn field(key: &str, value: Value) -> (String, Value) {
    (key.to_string(), value)
}

fn block(id: &str, title: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: title.into(),
        shape,
    }
}

/// A finished `dns.answers` block with the given root `diverge` and `unmeasured` counts.
fn answers(diverge: i64, unmeasured: i64) -> Block {
    let fields = vec![
        field("mode", text("query")),
        field("finished", Value::Bool(true)),
        field("record_types", Value::Int(1)),
        field("lookups", Value::Int(2)),
        field("servers", Value::Int(2)),
        field("agree", Value::Int(0)),
        field("diverge", Value::Int(diverge)),
        field("single", Value::Int(0)),
        field("unmeasured", Value::Int(unmeasured)),
        field("duration", Value::Duration(12)),
        field("transport", text("udp")),
    ];
    let child = Node {
        fields: vec![
            field("record_type", text("A")),
            field("agreement", text("agree")),
        ],
        children: vec![],
    };
    block(
        "dns.answers",
        "Answers",
        Shape::Tree(Node {
            fields,
            children: vec![child],
        }),
    )
}

/// A `dns.servers` block: one child per server, each with its `unmeasured` count.
fn servers(unmeasured: &[i64]) -> Block {
    let children = unmeasured
        .iter()
        .enumerate()
        .map(|(i, u)| Node {
            fields: vec![
                field("server", text(&format!("udp:192.0.2.{}:53", i + 1))),
                field("transport", text("udp")),
                field("lookups", Value::Int(4)),
                field("measured", Value::Int(4 - u)),
                field("unmeasured", Value::Int(*u)),
            ],
            children: vec![],
        })
        .collect();
    block(
        "dns.servers",
        "Servers",
        Shape::Tree(Node {
            fields: vec![field("servers", Value::Int(unmeasured.len() as i64))],
            children,
        }),
    )
}

fn render(blocks: &[Block]) -> String {
    let statuses = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    render_part(blocks, &ctx)
        .expect("the DNS part view claims a dns.answers block")
        .html
}

/// The inner HTML of the tab link carrying `data-tab="<tab>"`.
fn tab_link<'a>(html: &'a str, tab: &str) -> &'a str {
    let open = format!("data-tab=\"{tab}\">");
    let start = html
        .find(&open)
        .unwrap_or_else(|| panic!("no {tab} tab link: {html}"))
        + open.len();
    let len = html[start..].find("</a>").expect("tab link is closed");
    &html[start..start + len]
}

fn badge(tone: &str, label: &str) -> String {
    format!("<span class=\"dns-view__tab-badge\" data-tone=\"{tone}\">{label}</span>")
}

#[test]
fn c3_c8_c9_tab_badges_come_from_the_server_written_counts() {
    // (diverge, unmeasured per root, unmeasured per server) ->
    // (results badges present, results badges absent, servers badges present, servers absent)
    struct Row {
        name: &'static str,
        diverge: i64,
        unmeasured: i64,
        server_unmeasured: &'static [i64],
        results_has: Vec<String>,
        results_lacks: Vec<String>,
        servers_has: Vec<String>,
        servers_lacks: Vec<String>,
    }
    let pass = badge("pass", "✓");
    let rows = vec![
        Row {
            name: "diverge and unmeasured",
            diverge: 1,
            unmeasured: 1,
            server_unmeasured: &[0, 0],
            results_has: vec![badge("warn", "1 diverge"), badge("muted", "1 not measured")],
            results_lacks: vec![pass.clone()],
            servers_has: vec![pass.clone()],
            servers_lacks: vec![badge("warn", "not measured")],
        },
        Row {
            name: "all agree, one server unmeasured",
            diverge: 0,
            unmeasured: 0,
            server_unmeasured: &[1, 0],
            results_has: vec![pass.clone()],
            results_lacks: vec![
                badge("warn", "0 diverge"),
                "diverge</span>".to_string(),
                "not measured</span>".to_string(),
            ],
            servers_has: vec![badge("muted", "1 not measured")],
            servers_lacks: vec![pass.clone()],
        },
        Row {
            name: "counts are written, not derived",
            diverge: 3,
            unmeasured: 2,
            server_unmeasured: &[2, 1, 0],
            results_has: vec![badge("warn", "3 diverge"), badge("muted", "2 not measured")],
            results_lacks: vec![pass.clone()],
            servers_has: vec![badge("muted", "2 not measured")],
            servers_lacks: vec![pass.clone()],
        },
    ];

    for row in rows {
        let html = render(&[
            answers(row.diverge, row.unmeasured),
            servers(row.server_unmeasured),
        ]);
        let results = tab_link(&html, "results");
        let servers = tab_link(&html, "servers");
        for want in &row.results_has {
            assert!(results.contains(want), "{}: results {results}", row.name);
        }
        for not in &row.results_lacks {
            assert!(!results.contains(not), "{}: results {results}", row.name);
        }
        for want in &row.servers_has {
            assert!(servers.contains(want), "{}: servers {servers}", row.name);
        }
        for not in &row.servers_lacks {
            assert!(!servers.contains(not), "{}: servers {servers}", row.name);
        }
    }
}
