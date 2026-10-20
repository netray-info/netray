//! v2-tls-ui Phase 1 (C5, C7, C8, C19, C20, C21, C22): the certificate section and its facts grid.
use std::net::IpAddr;

use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::pages::views::{ViewContext, render_with};

fn t(s: &str) -> Value {
    Value::Text(s.into())
}

fn time(s: &str) -> Value {
    Value::Time(s.into())
}

fn fields(pairs: Vec<(&str, Value)>) -> Vec<(String, Value)> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn addr(s: &str) -> Value {
    Value::Addr(s.parse::<IpAddr>().unwrap())
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

/// The leaf's fields; `extra` replaces or adds keys.
fn leaf(extra: Vec<(&str, Value)>) -> Vec<(String, Value)> {
    let mut base = vec![
        ("position", Value::Int(0)),
        ("subject", t("CN=example.com")),
        ("issuer", t("CN=Example CA")),
        ("sans", t("example.com")),
        ("sans", t("www.example.com")),
        ("not_before", time("2026-10-07T00:00:00Z")),
        ("not_after", time("2027-01-06T00:00:00Z")),
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
    ];
    for (k, v) in extra {
        base.retain(|(bk, _)| *bk != k);
        base.push((k, v));
    }
    fields(base)
}

fn chain(ip: &str, leaf_fields: Vec<(String, Value)>) -> Block {
    Block {
        id: BlockId::parse(&format!("tls.chain@{ip}:443")).unwrap(),
        title: "Certificate chain".into(),
        shape: Shape::Chain(vec![leaf_fields]),
    }
}

fn handshake(ip: &str, extra: Vec<(&str, Value)>) -> Block {
    let mut pairs = vec![
        ("version", t("TLSv1.3")),
        ("cipher_suite", t("TLS_AES_256_GCM_SHA384")),
        ("handshake_ms", Value::Duration(22)),
    ];
    pairs.extend(extra);
    Block {
        id: BlockId::parse(&format!("tls.handshake@{ip}:443")).unwrap(),
        title: "Handshake".into(),
        shape: Shape::Fields(fields(pairs)),
    }
}

fn ct(ip: &str, n: i64) -> Block {
    Block {
        id: BlockId::parse("tls.ct").unwrap(),
        title: "Certificate Transparency".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: ["address", "sct_count", "log_ids"]
                .map(String::from)
                .to_vec(),
            rows: vec![vec![addr(ip), Value::Int(n), t("log-a, log-b, log-c")]],
        },
    }
}

fn render(siblings: &[Block]) -> String {
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    render_with(&siblings[0], &ctx, siblings).expect("tls.addresses is claimed")
}

/// One address, a chain, a handshake and optionally ct.
fn one(leaf_fields: Vec<(String, Value)>, hs: Vec<(&str, Value)>, with_ct: bool) -> String {
    let ip = "192.0.2.1";
    let mut blocks = vec![addresses(&[ip]), chain(ip, leaf_fields), handshake(ip, hs)];
    if with_ct {
        blocks.push(ct(ip, 3));
    }
    render(&blocks)
}

fn chip(tone: &str, text: &str) -> String {
    format!(r#"<span class="tls-chip" data-tone="{tone}">{text}</span>"#)
}

#[test]
fn c5_identical_chains_on_two_addresses_make_one_certificate_section() {
    let same = leaf(vec![]);
    let blocks = vec![
        addresses(&["192.0.2.1", "192.0.2.2"]),
        chain("192.0.2.1", same.clone()),
        chain("192.0.2.2", same),
        handshake("192.0.2.1", vec![]),
        handshake("192.0.2.2", vec![]),
    ];
    let html = render(&blocks);
    assert_eq!(html.matches(">Certificate<").count(), 1, "{html}");
    assert!(!html.contains("Certificate ·"), "{html}");
}

#[test]
fn c7_grid_lists_cells_in_order_and_chips_follow_the_flags() {
    let full = one(
        leaf(vec![
            ("serial", t("0A:1B")),
            ("fingerprint_sha1", t("AA:BB")),
            ("policy", t("OV")),
            ("ocsp_url", t("http://ocsp.example.com")),
            ("ca_issuers_url", t("http://ca.example.com/ca.crt")),
            ("self_signed", Value::Bool(true)),
            ("expired", Value::Bool(true)),
        ]),
        vec![],
        true,
    );
    assert!(full.contains(&chip("fail", "self-signed")), "{full}");
    assert!(full.contains(&chip("fail", "expired")), "{full}");
    for text in [
        "http://ocsp.example.com",
        "http://ca.example.com/ca.crt",
        "3 SCTs",
    ] {
        assert!(full.contains(text), "{text}: {full}");
    }
    let bare = one(leaf(vec![]), vec![], false);
    assert!(!bare.contains("self-signed"), "{bare}");
    assert!(!bare.contains(&chip("fail", "expired")), "{bare}");
    for label in [
        "Serial",
        "SHA-1",
        "Policy",
        "OCSP responder",
        "CA issuers",
        "SCT logs",
    ] {
        assert!(
            !bare.contains(&format!(r#"facts-grid__label">{label}<"#)),
            "{label}: {bare}"
        );
    }
    assert!(bare.contains(r#"facts-grid__label">SHA-256<"#), "{bare}");
}

#[test]
fn c8_ocsp_cell_shows_stapled_parts_freshness_and_the_live_chip_map() {
    let html = one(
        leaf(vec![]),
        vec![
            ("ocsp_stapled", Value::Bool(true)),
            ("ocsp_status", t("good")),
            ("ocsp_this_update", time("2026-10-08T00:00:00Z")),
            ("ocsp_next_update", time("2026-10-15T00:00:00Z")),
        ],
        false,
    );
    assert!(html.contains("stapled · good · 2026-10-08"), "{html}");
    assert!(html.contains("fresh until 2026-10-15"), "{html}");
    assert!(!html.contains("live:"), "{html}");
    for (status, tone) in [("good", "pass"), ("revoked", "fail"), ("unknown", "warn")] {
        let html = one(
            leaf(vec![]),
            vec![
                ("ocsp_stapled", Value::Bool(true)),
                ("ocsp_status", t("good")),
                ("ocsp_live_status", t(status)),
            ],
            false,
        );
        assert!(
            html.contains(&format!(
                r#"<span class="tls-chip" data-tone="{tone}">live: {status}"#
            )),
            "{status}: {html}"
        );
    }
}

#[test]
fn c19_labels_come_in_the_specified_order() {
    let html = one(
        leaf(vec![
            ("serial", t("0A:1B")),
            ("fingerprint_sha1", t("AA:BB")),
            ("policy", t("OV")),
            ("ocsp_url", t("http://ocsp.example.com")),
            ("ca_issuers_url", t("http://ca.example.com/ca.crt")),
        ]),
        vec![
            ("ocsp_stapled", Value::Bool(true)),
            ("ocsp_status", t("good")),
        ],
        true,
    );
    let start = html.find(">Certificate<").expect("certificate section");
    let grid = &html[start..];
    let labels = [
        "Subject",
        "Alternative names",
        "Issuer",
        "Key",
        "Signature",
        "OCSP",
        "Certificate Transparency",
        "Serial",
        "SHA-256",
        "SHA-1",
        "Policy",
        "OCSP responder",
        "CA issuers",
        "SCT logs",
    ];
    let mut last = 0;
    for label in labels {
        let needle = format!(r#"facts-grid__label">{label}<"#);
        let at = grid
            .find(&needle)
            .unwrap_or_else(|| panic!("{label} missing: {grid}"));
        assert!(at >= last, "{label} out of order: {grid}");
        last = at;
    }
}

#[test]
fn c20_without_policy_and_serial_neither_label_exists() {
    let html = one(leaf(vec![]), vec![], false);
    assert!(!html.contains(r#"facts-grid__label">Policy<"#), "{html}");
    assert!(!html.contains(r#"facts-grid__label">Serial<"#), "{html}");
}

#[test]
fn c21_a_good_stapled_response_reads_stapled_good_date() {
    let html = one(
        leaf(vec![]),
        vec![
            ("ocsp_stapled", Value::Bool(true)),
            ("ocsp_status", t("good")),
            ("ocsp_this_update", time("2026-10-08T00:00:00Z")),
        ],
        false,
    );
    assert!(html.contains("stapled · good · 2026-10-08"), "{html}");
}

#[test]
fn c22_a_revoked_live_status_reads_date_and_reason_and_no_key_means_no_chip() {
    let html = one(
        leaf(vec![]),
        vec![
            ("ocsp_stapled", Value::Bool(true)),
            ("ocsp_status", t("good")),
            ("ocsp_live_status", t("revoked")),
            ("ocsp_live_revoked_at", time("2026-10-01T00:00:00Z")),
            ("ocsp_live_reason", t("keyCompromise")),
        ],
        false,
    );
    assert!(
        html.contains(&chip("fail", "live: revoked 2026-10-01 · keyCompromise")),
        "{html}"
    );
    let none = one(
        leaf(vec![]),
        vec![("ocsp_stapled", Value::Bool(false))],
        false,
    );
    assert!(!none.contains("live:"), "{none}");
}
