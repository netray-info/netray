//! v2-ip-ui R11 (C4, C13): the user agent is its own section of the IP view: a "User agent"
//! heading, then Browser, Operating system, Device and the raw header as labelled cells, the
//! raw header with a copy button. No `ua_*` key, no section.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render};

const RAW: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

fn lookup_html(ua: &[(&str, &str)]) -> String {
    let mut fields = vec![(
        "address".to_string(),
        Value::Addr("192.0.2.10".parse().unwrap()),
    )];
    for flag in [
        "internal",
        "datacenter",
        "vpn",
        "tor",
        "bot",
        "c2",
        "spamhaus",
        "anycast",
        "cins",
    ] {
        fields.push((flag.into(), Value::Bool(false)));
    }
    for (key, value) in ua {
        fields.push(((*key).into(), Value::Text((*value).into())));
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

/// The markup following the cell label `label`, up to the next label (or the end).
fn cell<'a>(html: &'a str, label: &str) -> Option<&'a str> {
    let marker = format!("<span class=\"facts-grid__label\">{label}</span>");
    let rest = html.split(&marker).nth(1)?;
    Some(
        rest.split("<span class=\"facts-grid__label\">")
            .next()
            .unwrap(),
    )
}

fn firefox() -> Vec<(&'static str, &'static str)> {
    vec![
        ("ua_raw", RAW),
        ("ua_browser", "Firefox"),
        ("ua_browser_version", "128.0"),
        ("ua_os", "Linux"),
    ]
}

#[test]
fn c4_c13_user_agent_section_with_browser_os_device_and_raw_header() {
    let html = lookup_html(&firefox());
    assert!(html.contains("User agent"), "{html}");
    let browser = cell(&html, "Browser").unwrap_or_else(|| panic!("no Browser cell: {html}"));
    assert!(browser.contains("Firefox 128.0"), "{browser}");
    let os = cell(&html, "Operating system")
        .unwrap_or_else(|| panic!("no Operating system cell: {html}"));
    assert!(os.contains("Linux"), "{os}");
    assert!(!os.contains("Linux 6"), "{os}");
    assert!(cell(&html, "Device").is_none(), "{html}");

    let raw = cell(&html, "Raw header").unwrap_or_else(|| panic!("no Raw header cell: {html}"));
    assert!(raw.contains(RAW), "{raw}");
    let button = format!(
        "<button type=\"button\" class=\"inspector-copy\" data-copy-text=\"{RAW}\" hidden>"
    );
    assert!(raw.contains(&button), "{raw}");

    let mut ua = firefox();
    ua.push(("ua_os_version", "6.1"));
    ua.push(("ua_device", "Pixel"));
    let html = lookup_html(&ua);
    let os = cell(&html, "Operating system").expect("os cell");
    assert!(os.contains("Linux 6.1"), "{os}");
    let device = cell(&html, "Device").unwrap_or_else(|| panic!("no Device cell: {html}"));
    assert!(device.contains("Pixel"), "{device}");
}

#[test]
fn c13_without_user_agent_keys_no_section() {
    let html = lookup_html(&[]);
    assert!(!html.contains("User agent"), "{html}");
    assert!(cell(&html, "Raw header").is_none(), "{html}");
}
