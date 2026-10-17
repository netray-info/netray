//! Contract tests for whole-block volatile paths `<block>/*` (spec v2-history, Phase 5, requirement 18: C4, C7).
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::run::{Run, RunSection, SectionKind, fingerprint};
use netray_model::{Grade, Protocol};

fn t(s: &str) -> Value {
    Value::Text(s.into())
}

fn run(blocks: Vec<Block>) -> Run {
    Run {
        profile_version: 1,
        engine_version: "0.26.0".into(),
        evidence_schema_version: 1,
        grade: Grade::A,
        complete: true,
        sections: [Protocol::Email, Protocol::Tls]
            .into_iter()
            .map(|protocol| RunSection {
                protocol,
                outcome: SectionKind::Measured,
                reason: None,
            })
            .collect(),
        checks: vec![],
        blocks,
    }
}

fn table(id: &str, rows: &[(&str, &str)]) -> Block {
    Block {
        id: BlockId::parse(id).expect("valid block id"),
        title: "t".into(),
        shape: Shape::Table {
            key: "name".into(),
            columns: vec!["name".into(), "value".into()],
            rows: rows.iter().map(|(n, v)| vec![t(n), t(v)]).collect(),
        },
    }
}

/// Asserts the pair is invisible to the diff's evidence part and to the fingerprint.
fn assert_invisible(a: &Run, b: &Run, volatile: &[&str]) {
    let d = diff(a, b, volatile, None);
    assert!(
        d.evidence.is_empty(),
        "evidence must stay quiet: {:?}",
        d.evidence
    );
    assert_eq!(fingerprint(a, volatile), fingerprint(b, volatile));
}

#[test]
fn block_present_in_one_run_and_absent_in_the_other_is_neither_added_nor_removed() {
    let with = run(vec![table("email.mx_addresses", &[("mx1", "192.0.2.10")])]);
    let without = run(vec![]);
    let volatile = ["email.mx_addresses/*"];
    assert_invisible(&with, &without, &volatile);
    assert_invisible(&without, &with, &volatile);
}

#[test]
fn whole_block_content_and_qualified_blocks_stay_out_of_diff_and_fingerprint() {
    let a = run(vec![
        table("email.mx_addresses", &[("mx1", "192.0.2.10")]),
        table("tls.handshake@192.0.2.1:443", &[("protocol", "TLSv1.2")]),
    ]);
    let b = run(vec![
        table(
            "email.mx_addresses",
            &[("mx9", "198.51.100.7"), ("mx8", "203.0.113.5")],
        ),
        table("tls.handshake@192.0.2.1:443", &[("protocol", "TLSv1.3")]),
    ]);
    assert_invisible(&a, &b, &["email.mx_addresses/*", "tls.handshake/*"]);

    let absent = run(vec![]);
    assert_invisible(&a, &absent, &["email.mx_addresses/*", "tls.handshake/*"]);
}

#[test]
fn volatile_block_does_not_hide_a_change_in_another_block() {
    let a = run(vec![
        table("email.fcrdns", &[("mx1", "ok")]),
        table("email.dnsbl", &[("zone1", "clear")]),
    ]);
    let b = run(vec![
        table("email.fcrdns", &[("mx1", "changed")]),
        table("email.dnsbl", &[("zone1", "listed")]),
    ]);
    let volatile = ["email.fcrdns/*"];
    let d = diff(&a, &b, &volatile, None);
    assert_eq!(d.evidence.len(), 1, "only dnsbl: {:?}", d.evidence);
    assert_eq!(d.evidence[0].block.to_string(), "email.dnsbl");
    assert_ne!(fingerprint(&a, &volatile), fingerprint(&b, &volatile));
}
