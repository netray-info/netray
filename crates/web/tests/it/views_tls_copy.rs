//! v2-tls-ui Phase 5 (C16, C17, C18 server half): copy buttons on the leaf's facts and the
//! certificate-changed island's mount.
use std::net::IpAddr;

use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::pages::views::{ViewContext, render_with};

const SHA256: &str = "4F:9A:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:E1";
const SHA1: &str = "AA:BB:CC";
const ISSUER: &str = "CN=Example CA & Sons, O=<Ex>";

fn t(s: &str) -> Value {
    Value::Text(s.into())
}

fn fields(pairs: Vec<(&str, Value)>) -> Vec<(String, Value)> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn addresses(ip: &str) -> Block {
    Block {
        id: BlockId::parse("tls.addresses").unwrap(),
        title: "Addresses".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: ["address", "port", "outcome", "message"]
                .map(String::from)
                .to_vec(),
            rows: vec![vec![
                Value::Addr(ip.parse::<IpAddr>().unwrap()),
                Value::Int(443),
                t("ok"),
                t(""),
            ]],
        },
    }
}

fn leaf() -> Vec<(String, Value)> {
    fields(vec![
        ("position", Value::Int(0)),
        ("subject", t("CN=example.com")),
        ("issuer", t(ISSUER)),
        ("sans", t("example.com")),
        ("sans", t("www.example.com")),
        ("not_before", Value::Time("2026-10-07T00:00:00Z".into())),
        ("not_after", Value::Time("2027-01-06T00:00:00Z".into())),
        ("days_remaining", Value::Int(89)),
        ("key", t("ECDSA P-256")),
        ("signature_algorithm", t("ecdsa-with-SHA256")),
        ("serial", t("0A:1B")),
        ("fingerprint_sha256", t(SHA256)),
        ("fingerprint_sha1", t(SHA1)),
        ("ocsp_url", t("http://ocsp.example.com")),
        ("self_signed", Value::Bool(false)),
        ("expired", Value::Bool(false)),
    ])
}

fn page(ip: &str, sni: Option<&str>) -> String {
    let mut hs = vec![
        ("version", t("TLSv1.3")),
        ("cipher_suite", t("TLS_AES_256_GCM_SHA384")),
        ("handshake_ms", Value::Duration(22)),
        ("ocsp_stapled", Value::Bool(true)),
        ("ocsp_status", t("good")),
    ];
    if let Some(s) = sni {
        hs.push(("sni", t(s)));
    }
    let blocks = vec![
        addresses(ip),
        Block {
            id: BlockId::parse(&format!("tls.chain@{ip}:443")).unwrap(),
            title: "Certificate chain".into(),
            shape: Shape::Chain(vec![leaf()]),
        },
        Block {
            id: BlockId::parse(&format!("tls.handshake@{ip}:443")).unwrap(),
            title: "Handshake".into(),
            shape: Shape::Fields(fields(hs)),
        },
    ];
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    render_with(&blocks[0], &ctx, &blocks).expect("tls.addresses is claimed")
}

/// askama escapes `& < > " '` only.
fn unescape(s: &str) -> String {
    s.replace("&#60;", "<")
        .replace("&#62;", ">")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&#38;", "&")
}

/// The markup of one facts-grid cell: from its label to the next label (or the end).
fn cell<'a>(html: &'a str, label: &str) -> &'a str {
    let needle = format!(r#"facts-grid__label">{label}<"#);
    let start = html
        .find(&needle)
        .unwrap_or_else(|| panic!("{label} missing: {html}"));
    let rest = &html[start + needle.len()..];
    let end = rest.find("facts-grid__label").unwrap_or(rest.len());
    &rest[..end]
}

fn attr(tag: &str, name: &str) -> String {
    let needle = format!(r#" {name}=""#);
    let start = tag
        .find(&needle)
        .unwrap_or_else(|| panic!("{name} missing: {tag}"))
        + needle.len();
    let len = tag[start..].find('"').expect("attribute closes");
    unescape(&tag[start..start + len])
}

#[test]
fn c16_the_six_copy_cells_hold_a_hidden_copy_button_with_the_full_value() {
    let html = page("192.0.2.1", None);
    let want = [
        ("Subject", "CN=example.com"),
        ("Alternative names", "example.com\nwww.example.com"),
        ("Issuer", ISSUER),
        ("Serial", "0A:1B"),
        ("SHA-256", SHA256),
        ("SHA-1", SHA1),
    ];
    for (label, value) in want {
        let c = cell(&html, label);
        let open = r#"<button type="button" class="inspector-copy" data-copy-text=""#;
        let at = c
            .find(open)
            .unwrap_or_else(|| panic!("{label}: no copy button: {c}"));
        let tag_end = c[at..].find("</button>").expect("button closes") + "</button>".len();
        let tag = &c[at..at + tag_end];
        assert!(tag.ends_with(r#"" hidden>Copy</button>"#), "{label}: {tag}");
        assert_eq!(attr(tag, "data-copy-text"), value, "{label}: {tag}");
        assert_eq!(c.matches("inspector-copy").count(), 1, "{label}: {c}");
    }
    for label in ["Key", "Signature", "OCSP"] {
        let c = cell(&html, label);
        assert!(!c.contains("inspector-copy"), "{label}: {c}");
    }
}

#[test]
fn c17_the_certificate_section_mounts_certdiff_with_the_leaf_snapshot() {
    let html = page("192.0.2.1", Some("example.com"));
    let section = &html[html.find(">Certificate<").expect("certificate section")..];
    let open = r#"<div class="tls-certdiff" data-island="certdiff" data-cert-key=""#;
    let at = section
        .find(open)
        .unwrap_or_else(|| panic!("no certdiff mount: {section}"));
    let tag_end = section[at..].find('>').unwrap() + 1;
    let tag = &section[at..at + tag_end];
    assert!(tag.ends_with(" hidden>"), "{tag}");
    assert_eq!(attr(tag, "data-cert-key"), "example.com:443", "{tag}");
    assert_eq!(attr(tag, "data-not-after"), "2027-01-06T00:00:00Z", "{tag}");
    assert_eq!(attr(tag, "data-issuer"), ISSUER, "{tag}");
    assert_eq!(
        attr(tag, "data-sans"),
        "example.com\nwww.example.com",
        "{tag}"
    );
    assert_eq!(attr(tag, "data-sha256"), SHA256, "{tag}");
    let template = concat!(
        r#"<template class="tls-certdiff__template"><div class="tls-certdiff__banner" role="status">"#,
        r#"<p>Certificate changed since your last visit: <span class="tls-certdiff__changes"></span></p>"#,
        r#"<button type="button" class="tls-certdiff__forget">Forget</button></div></template></div>"#,
    );
    assert!(section[at + tag_end..].starts_with(template), "{section}");
}

#[test]
fn c17_without_sni_the_key_is_the_first_address_and_an_ipv6_one_is_bracketed() {
    let v4 = page("192.0.2.1", None);
    assert!(v4.contains(r#"data-cert-key="192.0.2.1:443""#), "{v4}");
    let v6 = page("2001:db8::1", None);
    assert!(v6.contains(r#"data-cert-key="[2001:db8::1]:443""#), "{v6}");
    let with_sni = page("2001:db8::1", Some("example.com"));
    assert!(
        with_sni.contains(r#"data-cert-key="example.com:443""#),
        "{with_sni}"
    );
}
