//! v2-tls-ui Phase 1 (C9, C23, C24): the chain pills, the inferred issuer pill, the trust
//! text and the crt.sh links.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, Status};
use netray_tls::validate::chain_trust::TRUST_STORE;
use netray_web::pages::views::{ViewContext, render_with};

const LEAF_SHA: &str = "4F:9A:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:01:02:03:04:05:06:07:08:09:0A:0B:0C:0D:E1";
const LEAF_SHA_PLAIN: &str = "4F9A00112233445566778899AABBCCDDEEFF0102030405060708090A0B0C0DE1";
const INT_SHA: &str = "A1:B2:C3:D4:E5:F6:07:18:29:3A:4B:5C:6D:7E:8F:90:A1:B2:C3:D4:E5:F6:07:18:29:3A:4B:5C:6D:7E:8F:91";
const INT_SERIAL: &str = "0DEADBEEF0123456789";

fn cert(
    subject: &str,
    issuer: &str,
    sha: &str,
    serial: &str,
    self_signed: bool,
) -> Vec<(String, Value)> {
    let t = |s: &str| Value::Text(s.to_string());
    vec![
        ("subject".into(), t(subject)),
        ("issuer".into(), t(issuer)),
        (
            "not_before".into(),
            Value::Time("2026-10-07T00:00:00Z".into()),
        ),
        (
            "not_after".into(),
            Value::Time("2027-01-06T00:00:00Z".into()),
        ),
        ("days_remaining".into(), Value::Int(89)),
        ("key".into(), t("ECDSA P-256")),
        ("signature_algorithm".into(), t("ecdsa-with-SHA256")),
        ("fingerprint_sha256".into(), t(sha)),
        ("serial".into(), t(serial)),
        ("self_signed".into(), Value::Bool(self_signed)),
        ("expired".into(), Value::Bool(false)),
    ]
}

fn blocks(chain: Vec<Vec<(String, Value)>>) -> Vec<Block> {
    vec![
        Block {
            id: BlockId::parse("tls.addresses").unwrap(),
            title: "Addresses".into(),
            shape: Shape::Table {
                key: "address".into(),
                columns: ["address", "port", "outcome", "message"]
                    .map(String::from)
                    .to_vec(),
                rows: vec![vec![
                    Value::Addr("192.0.2.1".parse().unwrap()),
                    Value::Int(443),
                    Value::Text("ok".into()),
                    Value::Text(String::new()),
                ]],
            },
        },
        Block {
            id: BlockId::parse("tls.chain@192.0.2.1:443").unwrap(),
            title: "Chain".into(),
            shape: Shape::Chain(chain),
        },
    ]
}

fn render(chain: Vec<Vec<(String, Value)>>, trusted: Option<Status>) -> String {
    let blocks = blocks(chain);
    let statuses: Vec<(CheckId, Status)> = trusted
        .map(|s| (CheckId::parse("tls.chain_trusted").unwrap(), s))
        .into_iter()
        .collect();
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &[],
    };
    render_with(&blocks[0], &ctx, &blocks).expect("tls.addresses is claimed")
}

fn leaf_and_intermediate() -> Vec<Vec<(String, Value)>> {
    vec![
        cert(
            "CN=example.com",
            "CN=Example Intermediate CA",
            LEAF_SHA,
            "01AA",
            false,
        ),
        cert(
            "CN=Example Intermediate CA",
            "CN=Example Root CA",
            INT_SHA,
            INT_SERIAL,
            false,
        ),
    ]
}

#[test]
fn c9_pills_in_served_order_with_an_inferred_issuer_and_trust_text() {
    let html = render(leaf_and_intermediate(), Some(Status::Pass));
    assert!(html.contains(r#"<ol class="tls-chain">"#), "{html}");
    let leaf = html.find("example.com</").expect("leaf pill");
    let inter = html
        .find("Example Intermediate CA</")
        .expect("intermediate pill");
    assert!(leaf < inter, "{html}");
    assert!(html.contains("→"), "{html}");
    assert_eq!(
        html.matches(r#"<li class="tls-chain__pill">"#).count(),
        2,
        "{html}"
    );
    assert_eq!(
        html.matches(r#"<li class="tls-chain__pill" data-inferred>"#)
            .count(),
        1,
        "{html}"
    );
    assert_eq!(
        html.matches(r#"class="tls-chain__crtsh""#).count(),
        2,
        "{html}"
    );
    assert!(
        html.contains(r#"<details class="tls-chain__more"><summary>Chain certificates</summary>"#),
        "{html}"
    );
    let trust = format!(
        r#"<span class="tls-chain__trust" data-tone="pass">trusted by {TRUST_STORE}</span>"#
    );
    assert!(html.contains(&trust), "{html}");
}

#[test]
fn c9_no_inferred_pill_when_the_last_served_certificate_is_self_signed() {
    let chain = vec![
        cert(
            "CN=example.com",
            "CN=Example Root CA",
            LEAF_SHA,
            "01AA",
            false,
        ),
        cert(
            "CN=Example Root CA",
            "CN=Example Root CA",
            INT_SHA,
            INT_SERIAL,
            true,
        ),
    ];
    let html = render(chain, Some(Status::Pass));
    assert!(html.contains(r#"<ol class="tls-chain">"#), "{html}");
    assert!(!html.contains("data-inferred"), "{html}");
    assert!(!html.contains("from the trust store"), "{html}");
}

#[test]
fn c9_trust_text_follows_the_check_status() {
    let fail = render(leaf_and_intermediate(), Some(Status::Fail));
    assert!(
        fail.contains(r#"<span class="tls-chain__trust" data-tone="fail">"#),
        "{fail}"
    );
    assert!(!fail.contains("trusted by"), "{fail}");
    let unknown = render(leaf_and_intermediate(), None);
    assert!(unknown.contains(r#"<ol class="tls-chain">"#), "{unknown}");
    assert!(!unknown.contains("tls-chain__trust"), "{unknown}");
}

#[test]
fn c23_the_leaf_crtsh_link_is_the_fingerprint_without_colons() {
    let html = render(leaf_and_intermediate(), Some(Status::Pass));
    let link = format!(
        r#"<a class="tls-chain__crtsh" href="https://crt.sh/?q={LEAF_SHA_PLAIN}">crt.sh ↗</a>"#
    );
    assert!(html.contains(&link), "{html}");
}

#[test]
fn c24_leaf_and_intermediate_show_the_inferred_issuer_and_the_chain_certificates() {
    let html = render(leaf_and_intermediate(), Some(Status::Pass));
    let inferred = html
        .split(r#"<li class="tls-chain__pill" data-inferred>"#)
        .nth(1)
        .expect("inferred pill")
        .split("</li>")
        .next()
        .unwrap();
    assert!(inferred.contains("Example Root CA"), "{inferred}");
    assert!(inferred.contains("from the trust store"), "{inferred}");
    assert!(
        html.contains(&format!("trusted by {TRUST_STORE}")),
        "{html}"
    );
    let more = html
        .split(r#"<details class="tls-chain__more">"#)
        .nth(1)
        .expect("chain certificates details")
        .split("</details>")
        .next()
        .unwrap();
    assert!(more.contains("Chain certificates"), "{more}");
    assert!(more.contains(INT_SERIAL), "{more}");
    assert!(more.contains(INT_SHA), "{more}");
}
