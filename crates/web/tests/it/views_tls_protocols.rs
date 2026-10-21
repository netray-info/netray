//! v2-tls-ui Phase 2 (C1, C4, C5, C6, C7, C8, C9): the protocols grid and the probe answers.
use std::net::IpAddr;

use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::pages::views::{ViewContext, render_with};

fn t(s: &str) -> Value {
    Value::Text(s.into())
}

fn fields(pairs: Vec<(&str, Value)>) -> Vec<(String, Value)> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn addr(s: &str) -> Value {
    Value::Addr(s.parse::<IpAddr>().unwrap())
}

fn fields_block(id: &str, title: &str, pairs: Vec<(&str, Value)>) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: title.into(),
        shape: Shape::Fields(fields(pairs)),
    }
}

fn addresses(ips: &[&str]) -> Block {
    Block {
        id: BlockId::parse("tls.addresses").unwrap(),
        title: "Addresses".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: ["address", "port", "outcome", "message"]
                .map(String::from)
                .to_vec(),
            rows: ips
                .iter()
                .map(|ip| vec![addr(ip), Value::Int(443), t("ok"), t("")])
                .collect(),
        },
    }
}

fn chain(ip: &str) -> Block {
    let leaf = fields(vec![
        ("position", Value::Int(0)),
        ("subject", t("CN=example.com")),
        ("issuer", t("CN=Example CA")),
        ("sans", t("example.com")),
        ("not_before", Value::Time("2026-10-07T00:00:00Z".into())),
        ("not_after", Value::Time("2027-01-06T00:00:00Z".into())),
        ("days_remaining", Value::Int(89)),
        ("key", t("ECDSA P-256")),
        ("signature_algorithm", t("ecdsa-with-SHA256")),
        (
            "fingerprint_sha256",
            t(
                "4F:9A:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:E1",
            ),
        ),
        ("self_signed", Value::Bool(false)),
        ("expired", Value::Bool(false)),
    ]);
    Block {
        id: BlockId::parse(&format!("tls.chain@{}:443", qual(ip))).unwrap(),
        title: "Certificate chain".into(),
        shape: Shape::Chain(vec![leaf]),
    }
}

fn handshake(ip: &str, extra: Vec<(&str, Value)>) -> Block {
    let mut pairs = vec![
        ("version", t("TLSv1.3")),
        ("cipher_suite", t("TLS_AES_256_GCM_SHA384")),
        ("handshake_ms", Value::Duration(22)),
    ];
    pairs.extend(extra);
    fields_block(
        &format!("tls.handshake@{}:443", qual(ip)),
        "Handshake",
        pairs,
    )
}

fn protocols(extra: Vec<(&str, Value)>) -> Block {
    fields_block("tls.protocols", "Protocols", extra)
}

fn full_protocols() -> Block {
    protocols(vec![
        ("tls1_3", t("enabled")),
        ("tls1_2", t("enabled")),
        ("tls1_1", t("disabled")),
        ("tls1_0", t("disabled")),
        ("forward_secrecy_all_suites", Value::Bool(true)),
    ])
}

fn render_with_statuses(blocks: &[Block], statuses: &[(CheckId, Status)]) -> String {
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses,
        decorate: &decorate,
        feeds: &[],
    };
    render_with(&blocks[0], &ctx, blocks).expect("tls.addresses is claimed")
}

fn render(blocks: &[Block]) -> String {
    render_with_statuses(blocks, &[])
}

fn port(hs: Vec<(&str, Value)>, proto: Option<Block>) -> Vec<Block> {
    let ip = "192.0.2.1";
    let mut blocks = vec![addresses(&[ip]), chain(ip), handshake(ip, hs)];
    blocks.extend(proto);
    blocks
}

const CELL: &str = r#"<div class="tls-protocols__cell""#;

/// The markup of the cell named `name`, from its opening tag to the next cell or the end.
fn cell(html: &str, name: &str) -> Option<String> {
    let needle = format!(r#"<span class="tls-protocols__name">{name}</span>"#);
    let at = html.find(&needle)?;
    let start = html[..at].rfind(CELL)?;
    let rest = &html[start + CELL.len()..];
    let end = rest
        .find(CELL)
        .map_or(html.len(), |e| start + CELL.len() + e);
    Some(html[start..end].to_string())
}

fn cell_names(html: &str) -> Vec<String> {
    let open = r#"<span class="tls-protocols__name">"#;
    html.match_indices(open)
        .map(|(i, _)| {
            let rest = &html[i + open.len()..];
            rest[..rest.find("</span>").unwrap()].to_string()
        })
        .collect()
}

fn state(c: &str) -> String {
    let open = r#"<span class="tls-protocols__state">"#;
    let at = c.find(open).unwrap_or_else(|| panic!("no state: {c}"));
    let rest = &c[at + open.len()..];
    rest[..rest.find("</span>").unwrap()].to_string()
}

fn id(s: &str) -> CheckId {
    CheckId::parse(s).unwrap()
}

#[test]
fn c1_taken_blocks_render_nothing_and_cells_follow_the_specified_order() {
    let probes = fields_block(
        "tls.probes@192.0.2.1:443",
        "Probes",
        vec![
            ("ssl3", t("refused")),
            ("ssl3_answer", t("handshake_failure alert")),
        ],
    );
    let mut blocks = port(
        vec![
            ("alpn", t("h2")),
            ("ech", Value::Bool(false)),
            ("sni", t("example.com")),
            ("starttls", t("smtp")),
        ],
        Some(protocols(vec![
            ("tls1_3", t("enabled")),
            ("tls1_2", t("enabled")),
            ("tls1_1", t("disabled")),
            ("tls1_0", t("disabled")),
            ("ssl3", t("disabled")),
            ("forward_secrecy_all_suites", Value::Bool(true)),
            ("suites_insecure", t("refused")),
            ("suites_cbc", t("accepted")),
            ("suites_static_rsa", t("refused")),
        ])),
    );
    blocks.push(probes);
    let html = render(&blocks);
    for taken in &blocks[3..] {
        let decorate = |_: &BlockId, _: &str, _: &Value| None;
        let ctx = ViewContext {
            statuses: &[],
            decorate: &decorate,
            feeds: &[],
        };
        assert_eq!(
            render_with(taken, &ctx, &blocks).as_deref(),
            Some(""),
            "{} renders nothing of its own",
            taken.id
        );
    }
    assert!(html.contains(r#"id="ev-tls.protocols""#), "{html}");
    let names = cell_names(&html);
    let expected = [
        "TLS 1.3",
        "TLS 1.2",
        "TLS 1.1",
        "TLS 1.0",
        "ALPN",
        "Forward secrecy",
        "Encrypted Client Hello",
        "HSTS",
        "SSL 3.0",
        "Cipher suites",
        "SNI",
        "STARTTLS",
    ];
    assert_eq!(names, expected, "{html}");
    assert_eq!(
        state(&cell(&html, "Cipher suites").unwrap()),
        "insecure refused · CBC accepted · static RSA refused"
    );
    assert!(html.contains("handshake_failure alert"), "{html}");
}

#[test]
fn c4_the_first_eight_cells_read_enabled_disabled_alpn_secrecy_ech_and_hsts() {
    let blocks = port(
        vec![
            ("alpn", t("h2")),
            ("ech", Value::Bool(false)),
            ("sni", t("example.com")),
        ],
        Some(full_protocols()),
    );
    let html = render(&blocks);
    let names = cell_names(&html);
    assert_eq!(
        &names[..8.min(names.len())],
        [
            "TLS 1.3",
            "TLS 1.2",
            "TLS 1.1",
            "TLS 1.0",
            "ALPN",
            "Forward secrecy",
            "Encrypted Client Hello",
            "HSTS"
        ],
        "{html}"
    );
    for (name, want) in [
        ("TLS 1.3", "enabled"),
        ("TLS 1.2", "enabled"),
        ("TLS 1.1", "disabled, as it should be"),
        ("TLS 1.0", "disabled, as it should be"),
        ("ALPN", "h2"),
        ("Forward secrecy", "all suites"),
        ("Encrypted Client Hello", "not advertised"),
    ] {
        let c = cell(&html, name).unwrap_or_else(|| panic!("{name}: {html}"));
        assert_eq!(state(&c), want, "{name}");
    }
    let hsts = cell(&html, "HSTS").expect("HSTS cell");
    assert!(
        hsts.contains(
            r#"<a href="/inspect/http?url=https://example.com/">checked in the HTTP inspector</a>"#
        ),
        "{hsts}"
    );
}

#[test]
fn c5_tones_come_from_the_checks_and_hsts_has_none() {
    let blocks = port(
        vec![
            ("alpn", t("h2")),
            ("ech", Value::Bool(false)),
            ("sni", t("example.com")),
        ],
        Some(full_protocols()),
    );
    let statuses = [
        (id("tls.ech_advertised"), Status::Warn),
        (id("tls.legacy_versions"), Status::Pass),
    ];
    let html = render_with_statuses(&blocks, &statuses);
    let tone = |name: &str, tone: &str| {
        let c = cell(&html, name).unwrap_or_else(|| panic!("{name}: {html}"));
        assert!(
            c.starts_with(&format!(r#"{CELL} data-tone="{tone}""#)),
            "{name}: {c}"
        );
    };
    tone("Encrypted Client Hello", "warn");
    tone("TLS 1.1", "pass");
    tone("TLS 1.0", "pass");
    let hsts = cell(&html, "HSTS").expect("HSTS cell");
    assert!(!hsts.contains("data-tone"), "{hsts}");
}

#[test]
fn c6_a_handshake_without_sni_has_no_sni_and_no_hsts_cell() {
    let blocks = port(vec![], Some(full_protocols()));
    let html = render(&blocks);
    assert!(cell(&html, "SNI").is_none(), "{html}");
    assert!(cell(&html, "HSTS").is_none(), "{html}");
    assert!(html.contains("tls-protocols__cell"), "{html}");
}

#[test]
fn c7_starttls_cell_reads_the_protocol_only_when_held() {
    let with = render(&port(vec![("starttls", t("smtp"))], Some(full_protocols())));
    let c = cell(&with, "STARTTLS").unwrap_or_else(|| panic!("STARTTLS cell: {with}"));
    assert_eq!(state(&c), "smtp");
    let without = render(&port(vec![], Some(full_protocols())));
    assert!(cell(&without, "STARTTLS").is_none(), "{without}");
    assert!(without.contains("tls-protocols__cell"), "{without}");
}

#[test]
fn c8_without_tls_protocols_no_version_cell_exists() {
    let html = render(&port(vec![("alpn", t("h2"))], None));
    for name in ["TLS 1.3", "TLS 1.2", "TLS 1.1", "TLS 1.0", "SSL 3.0"] {
        assert!(cell(&html, name).is_none(), "{name}: {html}");
    }
    assert!(!html.contains(r#"id="ev-tls.protocols""#), "{html}");
    assert!(cell(&html, "ALPN").is_some(), "{html}");
}

#[test]
fn c9_probe_answers_hold_a_header_per_address_and_each_word_and_answer() {
    let probe = |ip: &str, answer: &str| {
        fields_block(
            &format!("tls.probes@{}:443", qual(ip)),
            "Probes",
            vec![
                ("ssl3", t("refused")),
                ("ssl3_answer", t(answer)),
                ("tls1_0", t("refused")),
                ("tls1_0_answer", t("protocol_version alert")),
            ],
        )
    };
    let mut blocks = vec![
        addresses(&["192.0.2.1", "2001:db8::1"]),
        chain("192.0.2.1"),
        chain("2001:db8::1"),
        handshake("192.0.2.1", vec![]),
        handshake("2001:db8::1", vec![]),
        full_protocols(),
    ];
    blocks.push(probe("192.0.2.1", "handshake_failure alert"));
    blocks.push(probe("2001:db8::1", "ServerHello TLS 1.2"));
    let html = render(&blocks);
    let at = html
        .find(r#"<details class="tls-probes"><summary>Probe answers</summary>"#)
        .unwrap_or_else(|| panic!("no probe details: {html}"));
    let details = &html[at..];
    let details = &details[..details.find("</details>").unwrap_or(details.len())];
    for needle in [
        "<th>192.0.2.1</th>",
        "<th>2001:db8::1</th>",
        "handshake_failure alert",
        "ServerHello TLS 1.2",
        "protocol_version alert",
        "refused",
    ] {
        assert!(details.contains(needle), "{needle}: {details}");
    }
}

/// The address part of a block qualifier: an IPv6 address in brackets.
fn qual(ip: &str) -> String {
    if ip.contains(':') {
        format!("[{ip}]")
    } else {
        ip.to_string()
    }
}
