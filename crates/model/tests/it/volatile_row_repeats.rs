//! Contract tests for volatile row paths covering repeated keys `<key>#<n>` (spec v2-http-checks, C4, C17, C18).
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
        sections: vec![RunSection {
            protocol: Protocol::Http,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![],
        blocks,
    }
}

fn headers(rows: &[(&str, &str)]) -> Run {
    run(vec![Block {
        id: BlockId::parse("http.headers").expect("valid block id"),
        title: "t".into(),
        shape: Shape::Table {
            key: "name".into(),
            columns: vec!["name".into(), "value".into()],
            rows: rows.iter().map(|(n, v)| vec![t(n), t(v)]).collect(),
        },
    }])
}

const VOLATILE: [&str; 1] = ["http.headers/value@set-cookie"];

#[test]
fn volatile_row_path_covers_repeated_keys_in_fingerprint_and_diff() {
    let a = headers(&[
        ("set-cookie", "a=1"),
        ("set-cookie#2", "b=1"),
        ("set-cookie#3", "c=1"),
    ]);
    let b = headers(&[
        ("set-cookie", "a=2"),
        ("set-cookie#2", "b=2"),
        ("set-cookie#3", "c=2"),
    ]);
    let d = diff(&a, &b, &VOLATILE, None);
    assert!(
        d.evidence.is_empty(),
        "evidence must stay quiet: {:?}",
        d.evidence
    );
    assert_eq!(fingerprint(&a, &VOLATILE), fingerprint(&b, &VOLATILE));
}

#[test]
fn volatile_row_path_does_not_cover_lookalike_keys() {
    for key in ["set-cookiex", "set-cookie#a"] {
        let a = headers(&[(key, "one")]);
        let b = headers(&[(key, "two")]);
        let d = diff(&a, &b, &VOLATILE, None);
        assert!(!d.evidence.is_empty(), "{key}: diff must list the change");
        assert_ne!(
            fingerprint(&a, &VOLATILE),
            fingerprint(&b, &VOLATILE),
            "{key}: fingerprint must differ"
        );
    }
}
