//! Contract tests for the evidence rules: register shape kinds, id uniqueness, key uniqueness,
//! RFC 3339 times, canonical block ids, multi-valued fields and the evidence rule for checks.
use netray_model::evidence::{
    Block, BlockId, EvidenceError, Node, Record, Register, Shape, ShapeKind, Value, ValueType,
    check_evidence_rule, validate,
};
use netray_model::{CheckId, CheckResult, Status};
use serde_json::{json, to_value};

fn id(s: &str) -> BlockId {
    BlockId::parse(s).expect("valid block id")
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn block(name: &str, shape: Shape) -> Block {
    Block {
        id: id(name),
        title: "t".into(),
        shape,
    }
}

const REGISTER: Register = Register {
    blocks: &[
        ("x.records", ShapeKind::Records),
        ("x.table", ShapeKind::Table),
        ("x.chain", ShapeKind::Chain),
        ("x.tree", ShapeKind::Tree),
        ("x.fields", ShapeKind::Fields),
        ("x.text", ShapeKind::Text),
    ],
    keys: &[
        ("x.records", "owner", ValueType::Text),
        ("x.records", "type", ValueType::Text),
        ("x.records", "ttl", ValueType::Int),
        ("x.records", "data", ValueType::Text),
        ("x.table", "name", ValueType::Text),
        ("x.table", "value", ValueType::Text),
        ("x.chain", "sans", ValueType::Text),
        ("x.tree", "label", ValueType::Text),
        ("x.fields", "sans", ValueType::Text),
        ("x.fields", "seen", ValueType::Time),
    ],
};

fn check(blocks: Vec<Block>) -> Result<(), Vec<EvidenceError>> {
    validate("x", &REGISTER, &blocks)
}

fn record(ttl: Value) -> Record {
    Record {
        owner: text("example.com."),
        rtype: text("CAA"),
        ttl,
        data: text("0 issue \"ca.example.com\""),
    }
}

fn table(columns: &[&str], rows: Vec<Vec<Value>>) -> Shape {
    Shape::Table {
        key: "name".into(),
        columns: columns.iter().map(|c| c.to_string()).collect(),
        rows,
    }
}

#[test]
fn record_wire_form_is_tagged() {
    let wire = to_value(record(Value::Int(300))).unwrap();
    assert_eq!(
        wire,
        json!({
            "owner": {"t": "text", "v": "example.com."},
            "type": {"t": "text", "v": "CAA"},
            "ttl": {"t": "int", "v": 300},
            "data": {"t": "text", "v": "0 issue \"ca.example.com\""},
        })
    );
}

#[test]
fn validate_accepts_conforming_blocks_of_every_shape() {
    let blocks = vec![
        block("x.records", Shape::Records(vec![record(Value::Int(300))])),
        block(
            "x.table",
            table(
                &["name", "value"],
                vec![vec![text("a"), text("1")], vec![text("b"), text("2")]],
            ),
        ),
        block(
            "x.chain",
            Shape::Chain(vec![vec![("sans".into(), text("a"))]]),
        ),
        block(
            "x.tree",
            Shape::Tree(Node {
                fields: vec![("label".into(), text("root"))],
                children: vec![Node {
                    fields: vec![("label".into(), text("leaf"))],
                    children: vec![],
                }],
            }),
        ),
        block("x.fields", Shape::Fields(vec![("sans".into(), text("a"))])),
        block("x.text", Shape::Text("plain".into())),
    ];
    assert_eq!(check(blocks), Ok(()));
}

#[test]
fn validate_refuses_each_violation() {
    let cases: Vec<(&str, Vec<Block>)> = vec![
        (
            "records field mistyped",
            vec![block(
                "x.records",
                Shape::Records(vec![record(text("300"))]),
            )],
        ),
        (
            "table unregistered column",
            vec![block(
                "x.table",
                table(&["name", "bogus"], vec![vec![text("a"), text("b")]]),
            )],
        ),
        (
            "table short row",
            vec![block(
                "x.table",
                table(&["name", "value"], vec![vec![text("a")]]),
            )],
        ),
        (
            "table missing key column",
            vec![block("x.table", table(&["value"], vec![vec![text("a")]]))],
        ),
        (
            "table int cell in text column",
            vec![block(
                "x.table",
                table(&["name", "value"], vec![vec![text("a"), Value::Int(1)]]),
            )],
        ),
        (
            "table duplicate key values",
            vec![block(
                "x.table",
                table(
                    &["name", "value"],
                    vec![vec![text("a"), text("1")], vec![text("a"), text("2")]],
                ),
            )],
        ),
        (
            "chain unregistered key",
            vec![block(
                "x.chain",
                Shape::Chain(vec![vec![("bogus".into(), text("a"))]]),
            )],
        ),
        (
            "tree nested child mistyped",
            vec![block(
                "x.tree",
                Shape::Tree(Node {
                    fields: vec![("label".into(), text("root"))],
                    children: vec![Node {
                        fields: vec![("label".into(), Value::Int(1))],
                        children: vec![],
                    }],
                }),
            )],
        ),
        (
            "text block registered as fields",
            vec![block("x.fields", Shape::Text("plain".into()))],
        ),
        (
            "fields block registered as text",
            vec![block("x.text", Shape::Fields(vec![]))],
        ),
        (
            "duplicate block id",
            vec![
                block("x.text", Shape::Text("a".into())),
                block("x.text", Shape::Text("b".into())),
            ],
        ),
    ];
    for (name, blocks) in cases {
        assert!(check(blocks).is_err(), "{name} must be refused");
    }
}

#[test]
fn validate_checks_time_values_as_rfc3339() {
    let seen = |v: &str| {
        vec![block(
            "x.fields",
            Shape::Fields(vec![("seen".into(), Value::Time(v.into()))]),
        )]
    };
    for good in [
        "2026-12-01T00:00:00Z",
        "2030-04-01T00:00:00+00:00",
        "2026-12-01T01:02:03.5+02:00",
    ] {
        assert_eq!(check(seen(good)), Ok(()), "{good:?} must pass");
    }
    for bad in ["yesterday", "Apr  1 00:00:00 2030 +00:00", "2026-12-01", ""] {
        assert!(check(seen(bad)).is_err(), "{bad:?} must be refused");
    }
}

#[test]
fn validate_accepts_repeated_keys_in_fields_and_chain() {
    let two = vec![
        ("sans".to_string(), text("a")),
        ("sans".to_string(), text("b")),
    ];
    let blocks = vec![
        block("x.fields", Shape::Fields(two.clone())),
        block("x.chain", Shape::Chain(vec![two])),
    ];
    assert_eq!(check(blocks), Ok(()));
}

#[test]
fn block_id_canonicalises_the_qualifier() {
    let a = id("tls.chain@[2001:DB8::10]:443");
    let b = id("tls.chain@[2001:0db8:0:0:0:0:0:10]:443");
    assert_eq!(a.to_string(), "tls.chain@[2001:db8::10]:443");
    assert_eq!(b.to_string(), "tls.chain@[2001:db8::10]:443");
    assert_eq!(a, b);
}

#[test]
fn block_id_refuses_non_canonical_ports() {
    for bad in [
        "tls.tlsa@+443",
        "tls.tlsa@0443",
        "tls.tlsa@0",
        "tls.chain@192.0.2.1:+443",
        "tls.tlsa@65536",
    ] {
        assert!(BlockId::parse(bad).is_err(), "{bad:?} must be refused");
    }
}

fn result(status: Status, evidence: &[&str]) -> CheckResult {
    CheckResult {
        id: CheckId::parse("tls.caa_compliant").unwrap(),
        status,
        findings: vec![],
        evidence: evidence.iter().map(|s| id(s)).collect(),
    }
}

#[test]
fn evidence_rule_requires_resolving_references_for_judged_checks() {
    let blocks = [block("x.text", Shape::Text("plain".into()))];
    let judged = [Status::Pass, Status::Warn, Status::Fail];
    let unjudged = [Status::NotApplicable, Status::NotTested, Status::Unmeasured];

    for s in judged {
        assert!(
            check_evidence_rule(&[result(s, &[])], &blocks).is_err(),
            "{s:?} without reference"
        );
        assert!(
            check_evidence_rule(&[result(s, &["x.gone"])], &blocks).is_err(),
            "{s:?} dangling"
        );
        assert!(
            check_evidence_rule(&[result(s, &["x.text"])], &blocks).is_ok(),
            "{s:?} resolved"
        );
    }
    for s in unjudged {
        assert!(
            check_evidence_rule(&[result(s, &[])], &blocks).is_ok(),
            "{s:?} without reference"
        );
        assert!(
            check_evidence_rule(&[result(s, &["x.gone"])], &blocks).is_err(),
            "{s:?} dangling"
        );
        assert!(
            check_evidence_rule(&[result(s, &["x.text"])], &blocks).is_ok(),
            "{s:?} resolved"
        );
    }
}
