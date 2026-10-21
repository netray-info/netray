//! v2-ip-ui R12 (C5, C14): `ip.headers` is a closed disclosure of the proxy hops (an ordered
//! list) and the request headers (a table in block order); a block with an extra root key is
//! not claimed.
use netray_ip::module::headers_block;
use netray_model::evidence::{Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render};

fn s(v: &str) -> String {
    v.to_string()
}

fn headers() -> Vec<(String, String)> {
    vec![
        (s("accept"), s("text/html")),
        (s("user-agent"), s("test/1.0")),
        (s("x-example"), s("one")),
    ]
}

/// Positions of the needles in the text, each searched after the previous one.
fn in_order(html: &str, needles: &[&str]) {
    let mut from = 0;
    for needle in needles {
        let at = html[from..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing or out of order in: {html}"));
        from += at + needle.len();
    }
}

#[test]
fn c14_ip_headers_view_lists_hops_in_order_and_headers_in_block_order() {
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };

    let hops = [s("192.0.2.10"), s("198.51.100.7")];
    let html = render(&headers_block(&hops, &headers()), &ctx)
        .expect("the view must claim an ip.headers block");
    assert!(
        html.contains("id=\"ev-ip.headers\""),
        "anchor missing in: {html}"
    );
    assert!(html.contains("Request headers"), "heading missing: {html}");
    let details_at = html.find("<details").expect("no <details> element");
    let details_tag = &html[details_at..details_at + html[details_at..].find('>').unwrap()];
    assert!(
        !details_tag.split_whitespace().any(|a| a == "open"),
        "disclosure must be closed: {details_tag}"
    );
    assert!(html.contains("<ol"), "hop list missing: {html}");
    in_order(
        &html,
        &[
            "<ol",
            "<li",
            "192.0.2.10",
            "</li>",
            "<li",
            "198.51.100.7",
            "</li>",
        ],
    );
    assert!(
        !html.contains("no proxy hops"),
        "unexpected none text: {html}"
    );
    in_order(
        &html,
        &[
            "<table",
            "accept",
            "text/html",
            "user-agent",
            "test/1.0",
            "x-example",
            "one",
        ],
    );

    let none = render(&headers_block(&[], &headers()), &ctx).expect("claimed without hops");
    assert!(none.contains("no proxy hops"), "none text missing: {none}");
    assert!(!none.contains("<ol"), "no hop, no list: {none}");

    let mut extra = headers_block(&hops, &headers());
    let Shape::Tree(node) = &mut extra.shape else {
        panic!("ip.headers is a tree");
    };
    node.fields
        .push(("unknown_key".into(), Value::Text("x".into())));
    assert!(
        render(&extra, &ctx).is_none(),
        "a block with an extra root key must not be claimed"
    );
}
