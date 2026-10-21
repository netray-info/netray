//! v2-ip-ui R6 (C6, C18): the lookup card opens with the address and the reverse-DNS host
//! name or "no reverse DNS", a copy button beside each, the host linking to the DNS and TLS
//! inspectors.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render};

fn lookup_html(hostname: Option<&str>) -> String {
    let mut fields = vec![(
        "address".to_string(),
        Value::Addr("192.0.2.10".parse().unwrap()),
    )];
    if let Some(host) = hostname {
        fields.push(("hostname".to_string(), Value::Text(host.into())));
    }
    let block = Block {
        id: BlockId::parse("ip.lookup").unwrap(),
        title: "Lookup".into(),
        shape: Shape::Fields(fields),
    };
    let decorate = decorate(&[]);
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    render(&block, &ctx).expect("ip.lookup is a claimed block")
}

/// The `<button ...>` tags holding the class `inspector-copy`.
fn copy_buttons(html: &str) -> Vec<&str> {
    html.split("<button")
        .skip(1)
        .filter_map(|rest| rest.split('>').next())
        .filter(|tag| tag.contains("inspector-copy"))
        .collect()
}

fn has_hidden_copy(html: &str, text: &str) -> bool {
    let attr = format!("data-copy-text=\"{text}\"");
    copy_buttons(html)
        .iter()
        .any(|tag| tag.contains(&attr) && tag.split_whitespace().any(|w| w == "hidden"))
}

#[test]
fn c18_card_opens_with_address_and_host_each_with_a_copy_button() {
    let html = lookup_html(Some("stub.example.com"));
    assert!(
        html.contains("href=\"/inspect/dns?q=stub.example.com\""),
        "{html}"
    );
    assert!(
        html.contains("href=\"/inspect/tls?h=stub.example.com%3A443\""),
        "{html}"
    );
    assert!(has_hidden_copy(&html, "192.0.2.10"), "{html}");
    assert!(has_hidden_copy(&html, "stub.example.com"), "{html}");

    let html = lookup_html(None);
    assert!(html.contains("no reverse DNS"), "{html}");
    assert!(!html.contains("/inspect/dns"), "{html}");
}
