//! Contract tests for the run fingerprint (spec v2-history, Phase 1, requirement 3, C2 and C7-C13).
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, Record, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind, fingerprint};
use netray_model::{CheckId, Grade, Protocol, Status};

fn bid(s: &str) -> BlockId {
    BlockId::parse(s).expect("valid block id")
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn run(blocks: Vec<Block>) -> Run {
    let protocols = [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
    ];
    Run {
        profile_version: 1,
        engine_version: "0.26.0".into(),
        evidence_schema_version: 1,
        grade: Grade::A,
        complete: true,
        sections: protocols
            .into_iter()
            .map(|protocol| RunSection {
                protocol,
                outcome: SectionKind::Measured,
                reason: None,
            })
            .collect(),
        checks: vec![RunCheck {
            id: CheckId::parse("http.hsts").unwrap(),
            status: Status::Pass,
        }],
        blocks,
    }
}

fn headers(rows: &[(&str, &str)]) -> Block {
    Block {
        id: bid("http.headers"),
        title: "Response headers".into(),
        shape: Shape::Table {
            key: "name".into(),
            columns: vec!["name".into(), "value".into()],
            rows: rows.iter().map(|(n, v)| vec![text(n), text(v)]).collect(),
        },
    }
}

fn a_records(rows: &[(&str, i64)]) -> Block {
    Block {
        id: bid("dns.a"),
        title: "A".into(),
        shape: Shape::Records(
            rows.iter()
                .map(|(ip, ttl)| Record {
                    owner: text("example.com."),
                    rtype: text("A"),
                    ttl: Value::Int(*ttl),
                    data: text(ip),
                })
                .collect(),
        ),
    }
}

fn handshake(ms: u64) -> Block {
    Block {
        id: bid("tls.handshake@192.0.2.1:443"),
        title: "Handshake".into(),
        shape: Shape::Fields(vec![
            ("protocol".into(), text("TLSv1.3")),
            ("handshake_ms".into(), Value::Duration(ms)),
        ]),
    }
}

fn spf(prefixes: &[&str]) -> Block {
    let mut fields = vec![("record".to_string(), text("v=spf1 -all"))];
    fields.extend(
        prefixes
            .iter()
            .map(|p| ("authorized_prefixes".to_string(), text(p))),
    );
    Block {
        id: bid("email.spf"),
        title: "SPF".into(),
        shape: Shape::Fields(fields),
    }
}

fn csp(value: &str) -> Block {
    headers(&[("content-security-policy", value)])
}

#[test]
fn fingerprint_is_lowercase_hex_sha256_and_tracks_version_and_status() {
    let base = run(vec![headers(&[("server", "x")])]);
    let fp = fingerprint(&base, &[]);
    assert_eq!(fp.len(), 64);
    assert!(
        fp.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "{fp}"
    );
    assert_eq!(fp, fingerprint(&base.clone(), &[]));

    let mut other_profile = base.clone();
    other_profile.profile_version = 2;
    assert_ne!(fp, fingerprint(&other_profile, &[]));

    let mut other_status = base.clone();
    other_status.checks[0].status = Status::Fail;
    assert_ne!(fp, fingerprint(&other_status, &[]));
}

#[test]
fn volatile_header_and_record_order_leave_fingerprint_equal() {
    let a = run(vec![
        headers(&[("date", "Mon, 01 Dec 2025 00:00:00 GMT"), ("server", "x")]),
        a_records(&[("192.0.2.1", 300), ("192.0.2.2", 300)]),
    ]);
    let b = run(vec![
        headers(&[("date", "Tue, 02 Dec 2025 00:00:00 GMT"), ("server", "x")]),
        a_records(&[("192.0.2.2", 60), ("192.0.2.1", 60)]),
    ]);
    let volatile = ["http.headers/value@date", "dns.a/ttl"];
    assert_eq!(fingerprint(&a, &volatile), fingerprint(&b, &volatile));
}

#[test]
fn changed_hsts_header_changes_fingerprint() {
    let a = run(vec![headers(&[
        ("date", "Mon, 01 Dec 2025 00:00:00 GMT"),
        ("strict-transport-security", "max-age=31536000"),
    ])]);
    let b = run(vec![headers(&[
        ("date", "Mon, 01 Dec 2025 00:00:00 GMT"),
        ("strict-transport-security", "max-age=0"),
    ])]);
    let volatile = ["http.headers/value@date"];
    assert_ne!(fingerprint(&a, &volatile), fingerprint(&b, &volatile));
}

#[test]
fn volatile_field_path_is_removed_only_when_listed() {
    let a = run(vec![handshake(12)]);
    let b = run(vec![handshake(97)]);
    assert_eq!(
        fingerprint(&a, &["tls.handshake/handshake_ms"]),
        fingerprint(&b, &["tls.handshake/handshake_ms"])
    );
    assert_ne!(fingerprint(&a, &[]), fingerprint(&b, &[]));
}

#[test]
fn repeated_field_key_order_is_canonical_and_not_a_diff() {
    let a = run(vec![spf(&["192.0.2.0/24", "198.51.100.0/24"])]);
    let b = run(vec![spf(&["198.51.100.0/24", "192.0.2.0/24"])]);
    assert_eq!(fingerprint(&a, &[]), fingerprint(&b, &[]));
    assert!(diff(&a, &b, &[], None).evidence.is_empty());
}

#[test]
fn changed_repeated_field_value_changes_fingerprint() {
    let a = run(vec![spf(&["192.0.2.0/24", "198.51.100.0/24"])]);
    let b = run(vec![spf(&["192.0.2.0/24", "203.0.113.0/24"])]);
    assert_ne!(fingerprint(&a, &[]), fingerprint(&b, &[]));
}

#[test]
fn csp_nonce_is_normalised() {
    let a = run(vec![csp("default-src 'self'; script-src 'nonce-abc123'")]);
    let b = run(vec![csp("default-src 'self'; script-src 'nonce-zzz999'")]);
    assert_eq!(fingerprint(&a, &[]), fingerprint(&b, &[]));
    assert!(diff(&a, &b, &[], None).evidence.is_empty());
}

#[test]
fn csp_other_directive_change_changes_fingerprint() {
    let a = run(vec![csp("script-src 'self'")]);
    let b = run(vec![csp("script-src 'self' cdn.example.com")]);
    assert_ne!(fingerprint(&a, &[]), fingerprint(&b, &[]));
}

#[test]
fn unterminated_nonce_does_not_hide_other_sources() {
    let a = run(vec![csp(
        "script-src 'nonce-r4nd0m https://cdn-a.example.com 'strict-dynamic'",
    )]);
    let b = run(vec![csp(
        "script-src 'nonce-r4nd0m https://cdn-b.example.com 'strict-dynamic'",
    )]);
    assert_ne!(fingerprint(&a, &[]), fingerprint(&b, &[]));
    assert!(!diff(&a, &b, &[], None).evidence.is_empty());
}

#[test]
fn volatile_row_path_covers_the_whole_row() {
    let miss = run(vec![headers(&[("server", "example")])]);
    let hit = run(vec![headers(&[("server", "example"), ("age", "37")])]);
    let volatile = ["http.headers/value@age"];
    assert_eq!(fingerprint(&miss, &volatile), fingerprint(&hit, &volatile));
    assert!(diff(&miss, &hit, &volatile, None).evidence.is_empty());
    // the same pair without the path is a change
    assert_ne!(fingerprint(&miss, &[]), fingerprint(&hit, &[]));
}
