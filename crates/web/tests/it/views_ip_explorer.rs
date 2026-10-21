//! v2-ip-ui R13 (C1, C3): after the lookup card the IP view offers "Use it from the command
//! line": an open `<details>` with one row per command (the text in a `<code>`, a hidden copy
//! button carrying it) and a link to the API docs.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render};

fn lookup_html(address: &str) -> String {
    let block = Block {
        id: BlockId::parse("ip.lookup").unwrap(),
        title: "Lookup".into(),
        shape: Shape::Fields(vec![(
            "address".to_string(),
            Value::Addr(address.parse().unwrap()),
        )]),
    };
    let decorate = decorate(&[]);
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    render(&block, &ctx).expect("ip.lookup is a claimed block")
}

/// Askama's text escaping.
fn esc(s: &str) -> String {
    s.replace('&', "&#38;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

fn commands(address: &str) -> Vec<String> {
    vec![
        "curl ip.netray.info".to_string(),
        "curl ip.netray.info/json".to_string(),
        "curl ip.netray.info/country".to_string(),
        format!("curl \"ip.netray.info/?ip={address}\""),
        format!("curl \"https://netray.info/api/v1/ip/{address}?format=yaml\""),
        "curl \"https://netray.info/api/v1/ip?fields=ip,location&format=json\"".to_string(),
        format!(
            "curl https://netray.info/api/v1/ip/batch --json '{{\"addresses\":[\"{address}\"]}}'"
        ),
    ]
}

/// Inner texts of every `<code ...>` element, trimmed.
fn code_texts(html: &str) -> Vec<&str> {
    html.split("<code")
        .skip(1)
        .filter_map(|rest| rest.split_once('>')?.1.split("</code>").next())
        .map(str::trim)
        .collect()
}

fn has_hidden_copy(html: &str, text: &str) -> bool {
    let attr = format!("data-copy-text=\"{text}\"");
    html.split("<button")
        .skip(1)
        .filter_map(|rest| rest.split('>').next())
        .filter(|tag| tag.contains("inspector-copy"))
        .any(|tag| tag.contains(&attr) && tag.split_whitespace().any(|w| w == "hidden"))
}

#[test]
fn c1_c3_command_line_section_lists_each_command_with_a_copy_button() {
    for address in ["192.0.2.10", "2001:db8::1"] {
        let html = lookup_html(address);
        assert!(html.contains("Use it from the command line"), "{html}");
        assert!(
            html.split("<details")
                .skip(1)
                .filter_map(|rest| rest.split('>').next())
                .any(|tag| tag.split_whitespace().any(|w| w == "open")),
            "{html}"
        );
        assert!(html.contains("href=\"/api/v1/docs\""), "{html}");
        let texts = code_texts(&html);
        for command in commands(address) {
            let escaped = esc(&command);
            assert!(
                texts.contains(&escaped.as_str()),
                "no <code> holding {escaped} for {address}: {html}"
            );
            assert!(
                has_hidden_copy(&html, &escaped),
                "no hidden copy button for {escaped} for {address}: {html}"
            );
        }
    }
}
