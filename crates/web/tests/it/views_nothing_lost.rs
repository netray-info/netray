//! v2-http-ui Phase 2 (C13): a claimed block shows every registered key's value; the views
//! drop nothing the generic evidence block would have shown.
use std::collections::BTreeSet;

use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::evidence::display_value;
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render};

/// One fixture block, the texts that must appear in its fragment, and every key it stores.
struct Row {
    block: Block,
    needles: Vec<String>,
    keys: BTreeSet<String>,
}

/// askama's escaping of a text value.
fn escaped(text: &str) -> String {
    // askama 0.16's HTML escaping.
    text.replace('&', "&#38;")
        .replace('<', "&#60;")
        .replace('>', "&#62;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

impl Row {
    fn new(id: &str, shape: impl FnOnce(&mut Row) -> Shape) -> Row {
        let mut row = Row {
            block: Block {
                id: BlockId::parse(id).unwrap(),
                title: id.into(),
                shape: Shape::Text(String::new()),
            },
            needles: vec![],
            keys: BTreeSet::new(),
        };
        row.block.shape = shape(&mut row);
        row
    }

    /// A stored pair whose display text, escaped, must appear.
    fn pair(&mut self, key: &str, value: Value) -> (String, Value) {
        self.needles.push(escaped(&display_value(&value)));
        self.keys.insert(key.to_string());
        (key.to_string(), value)
    }

    /// A stored pair a view shows in its own words: `needle` must appear instead.
    fn pair_shown_as(&mut self, key: &str, value: Value, needle: &str) -> (String, Value) {
        self.needles.push(escaped(needle));
        self.keys.insert(key.to_string());
        (key.to_string(), value)
    }
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

/// The rows of Phases 2 and 3.
fn rows() -> Vec<Row> {
    vec![
        Row::new("http.caching", |r| {
            let fields = vec![
                r.pair("directive", text("public")),
                r.pair_shown_as("max_age", Value::Int(3600), "3600"),
                r.pair("etag", text("W/\"abc\"")),
                r.pair_shown_as("etag_weak", Value::Bool(true), "weak"),
                r.pair_shown_as(
                    "last_modified",
                    Value::Time("2024-01-01T00:00:00Z".into()),
                    "2024-01-01 00:00 UTC",
                ),
                r.pair("vary", text("Accept-Encoding")),
                r.pair_shown_as("age", Value::Int(42), "42"),
            ];
            Shape::Fields(fields)
        }),
        Row::new("http.cdn", |r| {
            let fields = vec![
                r.pair("provider", text("Example CDN")),
                r.pair("cache_status", text("HIT")),
                r.pair("indicator", text("x-example-cache")),
            ];
            Shape::Fields(fields)
        }),
        Row::new("http.csp", |r| {
            let fields = vec![
                r.pair_shown_as("report_only", Value::Bool(true), "Report-Only"),
                r.pair("issue_count", Value::Int(2)),
                r.pair("issue", text("Missing base-uri directive")),
                r.pair("issue", text("object-src allows <object> & 'unsafe-eval'")),
            ];
            let child = vec![
                r.pair("directive", text("script-src")),
                r.pair("source", text("'self'")),
                r.pair("source", text("https://cdn.example.com")),
                r.pair("issue", text("script-src contains 'unsafe-inline'")),
            ];
            Shape::Tree(Node {
                fields,
                children: vec![Node {
                    fields: child,
                    children: vec![],
                }],
            })
        }),
        Row::new("http.hsts", |r| {
            let fields = vec![
                r.pair_shown_as("max_age", Value::Int(63_072_000), "63072000"),
                r.pair("include_subdomains", Value::Bool(true)),
                r.pair("preload", Value::Bool(true)),
            ];
            Shape::Fields(fields)
        }),
        Row::new("http.isolation", |r| {
            let fields = vec![
                r.pair("coop", text("same-origin")),
                r.pair("coep", text("require-corp")),
                r.pair("corp", text("same-site")),
                r.pair_shown_as("missing", text("cross-origin-opener-policy"), "missing"),
                r.pair_shown_as("missing", text("cross-origin-embedder-policy"), "missing"),
                r.pair_shown_as("missing", text("cross-origin-resource-policy"), "missing"),
            ];
            Shape::Fields(fields)
        }),
        Row::new("http.server", |r| {
            let fields = vec![
                r.pair("final_url", text("https://www.example.com/a?b=1&c=2")),
                r.pair("status_code", Value::Int(200)),
                r.pair("http_version", text("HTTP/2")),
                r.pair("inspection_ms", Value::Duration(1_200)),
                r.pair("address", Value::Addr("192.0.2.7".parse().unwrap())),
                r.pair("org", text("Example Hosting Ltd")),
                r.pair("network_type", text("hosting")),
                r.pair("error_kind", text("tls_handshake")),
            ];
            Shape::Fields(fields)
        }),
    ]
}

#[test]
fn c13_claimed_blocks_show_every_registered_value() {
    let register = &netray_http::module::REGISTER;
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    for row in rows() {
        let id = row.block.id.to_string();

        let registered: BTreeSet<String> = register
            .keys
            .iter()
            .filter(|(block, _, _)| *block == id)
            .map(|(_, key, _)| key.to_string())
            .collect();
        assert_eq!(
            row.keys, registered,
            "{id}: fixture must hold every registered key"
        );

        let html = render(&row.block, &ctx)
            .unwrap_or_else(|| panic!("{id}: the view must claim a block of registered keys"));
        for needle in &row.needles {
            assert!(
                html.contains(needle.as_str()),
                "{id}: {needle:?} missing in: {html}"
            );
        }
    }
}
