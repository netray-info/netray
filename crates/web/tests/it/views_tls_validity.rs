//! v2-tls-ui Phase 1 (C6, C16, C17, C18): the validity bar's layout, its rendered states and
//! the expiry window's single threshold.
use std::time::{SystemTime, UNIX_EPOCH};

use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_tls::quality::checks::{EXPIRY_WARN_DAYS, check_expiry_window};
use netray_tls::tls::chain::CertInfo;
use netray_tls::validate::CheckStatus;
use netray_web::pages::views::validity::{ValidityState, layout};
use netray_web::pages::views::{ViewContext, render_with};

const DAY: i64 = 86_400;
const ADDR: &str = "tls.chain@192.0.2.1:443";

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// (year, month, day) of a unix day count, civil-from-days.
fn ymd(secs: i64) -> (i64, i64, i64) {
    let z = secs.div_euclid(DAY) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn date(secs: i64) -> String {
    let (y, m, d) = ymd(secs);
    format!("{y:04}-{m:02}-{d:02}")
}

fn rfc3339(secs: i64) -> String {
    let (y, m, d) = ymd(secs);
    let s = secs.rem_euclid(DAY);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

fn blocks(not_before: i64, not_after: i64) -> Vec<Block> {
    let t = |k: &str, v: Value| (k.to_string(), v);
    let leaf = vec![
        t("position", Value::Text("leaf".into())),
        t("subject", Value::Text("CN=example.com".into())),
        t("issuer", Value::Text("CN=Example CA".into())),
        t("not_before", Value::Time(rfc3339(not_before))),
        t("not_after", Value::Time(rfc3339(not_after))),
        t(
            "days_remaining",
            Value::Int((not_after - now()).div_euclid(DAY)),
        ),
        t("key", Value::Text("ECDSA P-256".into())),
        t("self_signed", Value::Bool(false)),
        t("expired", Value::Bool(not_after < now())),
    ];
    let addresses = Block {
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
    };
    let chain = Block {
        id: BlockId::parse(ADDR).unwrap(),
        title: "Certificate chain".into(),
        shape: Shape::Chain(vec![leaf]),
    };
    let handshake = Block {
        id: BlockId::parse("tls.handshake@192.0.2.1:443").unwrap(),
        title: "Handshake".into(),
        shape: Shape::Fields(vec![
            t("version", Value::Text("TLSv1.3".into())),
            t("cipher_suite", Value::Text("TLS_AES_128_GCM_SHA256".into())),
            t("handshake_ms", Value::Duration(22)),
        ]),
    };
    vec![addresses, chain, handshake]
}

fn view(not_before: i64, not_after: i64) -> String {
    let all = blocks(not_before, not_after);
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    render_with(&all[0], &ctx, &all).expect("tls.addresses is claimed")
}

#[test]
fn c16_layout_places_marker_and_warn_zone_in_viewbox_units() {
    let l = layout(
        "2026-10-07T00:00:00Z",
        "2027-01-06T00:00:00Z",
        1_791_504_000,
    )
    .unwrap();
    assert_eq!(l.total_days, 91);
    assert_eq!(l.elapsed_days, 2);
    assert_eq!(l.days_left, 89);
    assert_eq!(l.warn_days, 30);
    assert_eq!(l.marker_x, 22);
    assert_eq!(l.warn_x, 670);
    assert_eq!(l.state, ValidityState::Valid);
}

#[test]
fn c16_valid_certificate_shows_labels_and_caption() {
    let n = now();
    let html = view(n - 10 * DAY - 3600, n + 80 * DAY + 3600);
    assert!(
        html.contains(r#"<div class="tls-validity" data-state="valid">"#),
        "{html}"
    );
    for label in [
        format!("not before {}", date(n - 10 * DAY - 3600)),
        format!("not after {}", date(n + 80 * DAY + 3600)),
        "today · 80 days left".to_string(),
    ] {
        assert!(
            html.contains(&format!(
                r#"<span class="tls-validity__label">{label}</span>"#
            )),
            "{label}: {html}"
        );
    }
    assert!(
        html.contains(
            r#"<p class="tls-validity__caption">the last 30 days turn the expiry line amber</p>"#
        ),
        "{html}"
    );
}

#[test]
fn c17_expired_leaf_renders_expired_state_and_days_ago() {
    let n = now();
    let html = view(n - 100 * DAY, n - 12 * DAY - 3600);
    assert!(html.contains(r#"data-state="expired""#), "{html}");
    assert!(html.contains("expired 12 days ago"), "{html}");
}

#[test]
fn c6_not_yet_valid_certificate_renders_its_state_and_label() {
    let n = now();
    let html = view(n + 5 * DAY + 3600, n + 95 * DAY);
    assert!(html.contains(r#"data-state="not-yet-valid""#), "{html}");
    assert!(html.contains("not valid for 5 days"), "{html}");
    let l = layout(
        "2026-10-20T00:00:00Z",
        "2027-01-20T00:00:00Z",
        1_791_244_800,
    )
    .unwrap();
    assert_eq!(l.state, ValidityState::NotYetValid);
}

fn cert(days_remaining: i64) -> CertInfo {
    CertInfo {
        position: "leaf".into(),
        subject: "CN=example.com".into(),
        issuer: "CN=Example CA".into(),
        sans: vec!["example.com".into()],
        serial: "01".into(),
        not_before: "2026-01-01T00:00:00Z".into(),
        not_after: "2027-01-01T00:00:00Z".into(),
        days_remaining,
        key_type: "ECDSA P-256".into(),
        key_size: 256,
        signature_algorithm: "ecdsa-with-SHA256".into(),
        fingerprint_sha256: "AA".into(),
        fingerprint_sha1: "BB".into(),
        lifetime_days: 90,
        is_expired: false,
        is_self_signed: false,
        cert_policy: "DV".into(),
        ocsp_url: None,
        ca_issuers_url: None,
    }
}

#[test]
fn c18_expiry_window_compares_against_expiry_warn_days() {
    assert_eq!(EXPIRY_WARN_DAYS, 30);
    assert_eq!(
        check_expiry_window(&[cert(EXPIRY_WARN_DAYS)]).status,
        CheckStatus::Warn
    );
    assert_eq!(
        check_expiry_window(&[cert(EXPIRY_WARN_DAYS + 1)]).status,
        CheckStatus::Pass
    );
}
