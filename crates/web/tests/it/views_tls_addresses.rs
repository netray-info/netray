//! v2-tls-ui Phase 1 (C3, C4, C11, C14, C15): the TLS port view's addresses table,
//! distinct-value cells, error rows and per-leaf certificate sections.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::pages::views::{ViewContext, render_with};

const FP_A: &str = "4F:9A:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:D0:E1";
const FP_B: &str = "AB:CD:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:D0:F2";

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn addresses(rows: &[(&str, &str, &str)]) -> Block {
    Block {
        id: BlockId::parse("tls.addresses").unwrap(),
        title: "Addresses".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: ["address", "port", "outcome", "message"]
                .map(String::from)
                .to_vec(),
            rows: rows
                .iter()
                .map(|(a, o, m)| {
                    vec![
                        Value::Addr(a.parse().unwrap()),
                        Value::Int(443),
                        text(o),
                        text(m),
                    ]
                })
                .collect(),
        },
    }
}

fn chain(addr: &str, fp: &str) -> Block {
    Block {
        id: BlockId::parse(&format!("tls.chain@{addr}:443")).unwrap(),
        title: "Chain".into(),
        shape: Shape::Chain(vec![vec![
            ("position".into(), Value::Int(0)),
            ("subject".into(), text("CN=example.com")),
            ("issuer".into(), text("CN=Example CA")),
            (
                "not_before".into(),
                Value::Time("2026-10-07T00:00:00Z".into()),
            ),
            (
                "not_after".into(),
                Value::Time("2027-01-06T00:00:00Z".into()),
            ),
            ("days_remaining".into(), Value::Int(89)),
            ("fingerprint_sha256".into(), text(fp)),
            ("self_signed".into(), Value::Bool(false)),
            ("expired".into(), Value::Bool(false)),
        ]]),
    }
}

fn handshake(addr: &str, ms: u64, ocsp_status: &str) -> Block {
    Block {
        id: BlockId::parse(&format!("tls.handshake@{addr}:443")).unwrap(),
        title: "Handshake".into(),
        shape: Shape::Fields(vec![
            ("version".into(), text("TLSv1.3")),
            ("cipher_suite".into(), text("TLS_AES_256_GCM_SHA384")),
            ("ocsp_stapled".into(), Value::Bool(true)),
            ("ocsp_status".into(), text(ocsp_status)),
            ("handshake_ms".into(), Value::Duration(ms)),
        ]),
    }
}

fn view(blocks: &[Block], statuses: &[(netray_model::CheckId, netray_model::Status)]) -> String {
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses,
        decorate: &decorate,
        feeds: &[],
    };
    render_with(&blocks[0], &ctx, blocks).expect("tls.addresses is taken by the port view")
}

fn two_addresses(fp2: &str, ms: (u64, u64), ocsp: (&str, &str)) -> Vec<Block> {
    vec![
        addresses(&[("192.0.2.1", "ok", ""), ("192.0.2.2", "ok", "")]),
        chain("192.0.2.1", FP_A),
        handshake("192.0.2.1", ms.0, ocsp.0),
        chain("192.0.2.2", fp2),
        handshake("192.0.2.2", ms.1, ocsp.1),
    ]
}

#[test]
fn c3_equal_values_show_once_and_differing_values_show_their_addresses() {
    let same = view(&two_addresses(FP_A, (22, 22), ("good", "good")), &[]);
    assert!(same.contains("stapled · good"), "{same}");
    assert_eq!(same.matches("stapled · good").count(), 1, "{same}");
    assert!(same.contains("Port 443 · handshake 22 ms<"), "{same}");

    let differ = view(&two_addresses(FP_A, (22, 31), ("good", "revoked")), &[]);
    assert!(differ.contains("stapled · good (192.0.2.1)"), "{differ}");
    assert!(differ.contains("stapled · revoked (192.0.2.2)"), "{differ}");
}

#[test]
fn c4_addresses_table_has_leaf_summary_and_same_marker() {
    let html = view(&two_addresses(FP_A, (22, 22), ("good", "good")), &[]);
    assert!(html.contains(r#"<table class="tls-addresses">"#), "{html}");
    for th in ["Address", "Band", "Protocol", "Cipher", "Leaf"] {
        assert!(html.contains(&format!(">{th}<")), "{th}: {html}");
    }
    assert!(html.contains("SHA256 4f:9a:…:e1"), "{html}");
    assert!(html.contains(&format!(r#"title="{FP_A}""#)), "{html}");
    assert!(html.contains("TLSv1.3"), "{html}");
    assert!(html.contains("TLS_AES_256_GCM_SHA384"), "{html}");
    assert_eq!(
        html.matches(r#"<span class="tls-addresses__leaf" data-tone="pass">same</span>"#)
            .count(),
        2,
        "{html}"
    );
    assert!(!html.contains("differs"), "{html}");
    let table = &html[html.find("<table").unwrap()..html.find("</table>").unwrap()];
    assert!(
        !table.contains("<svg"),
        "no statuses, no mini bars: {table}"
    );

    let one = view(
        &[
            addresses(&[("192.0.2.1", "ok", "")]),
            chain("192.0.2.1", FP_A),
            handshake("192.0.2.1", 22, "good"),
        ],
        &[],
    );
    assert!(
        !one.contains("tls-addresses__leaf"),
        "a lone row shows no marker: {one}"
    );
}

#[test]
fn c11_a_different_leaf_differs_and_gets_its_own_certificate_section() {
    let html = view(&two_addresses(FP_B, (22, 22), ("good", "good")), &[]);
    assert!(
        html.contains(r#"<span class="tls-addresses__leaf" data-tone="pass">same</span>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<span class="tls-addresses__leaf" data-tone="warn">differs</span>"#),
        "{html}"
    );
    assert!(html.contains("Certificate · 192.0.2.1"), "{html}");
    assert!(html.contains("Certificate · 192.0.2.2"), "{html}");
}

#[test]
fn c14_an_error_row_shows_outcome_and_message_across_the_remaining_columns() {
    let blocks = vec![
        addresses(&[
            ("192.0.2.1", "ok", ""),
            (
                "192.0.2.2",
                "HANDSHAKE_TIMEOUT",
                "handshake timed out after 5 s",
            ),
        ]),
        chain("192.0.2.1", FP_A),
        handshake("192.0.2.1", 22, "good"),
    ];
    let html = view(&blocks, &[]);
    let at = html
        .find(r#"<td class="tls-addresses__error" colspan="4">"#)
        .unwrap_or_else(|| panic!("{html}"));
    let cell = &html[at..at + html[at..].find("</td>").unwrap()];
    assert!(cell.contains("HANDSHAKE_TIMEOUT"), "{cell}");
    assert!(cell.contains("handshake timed out after 5 s"), "{cell}");
}

#[test]
fn c15_the_note_lists_each_address_handshake_duration() {
    let html = view(&two_addresses(FP_A, (22, 31), ("good", "good")), &[]);
    assert!(
        html.contains(
            r#"<p class="protocol-view__note">Port 443 · handshake 22 ms (192.0.2.1) · 31 ms (192.0.2.2)</p>"#
        ),
        "{html}"
    );
}
