//! Contract tests for the stored run (spec v2-history, Phase 1, C1 and C6).
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Node, Record, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use serde_json::{json, to_value};
use std::net::IpAddr;

fn block(name: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(name).expect("valid block id"),
        title: "t".into(),
        shape,
    }
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn run_with(blocks: Vec<Block>) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::APlus,
        complete: false,
        sections: vec![
            RunSection {
                protocol: Protocol::Dns,
                outcome: SectionKind::Measured,
                reason: None,
            },
            RunSection {
                protocol: Protocol::Tls,
                outcome: SectionKind::TimedOut,
                reason: Some("no answer from 192.0.2.1".into()),
            },
        ],
        checks: vec![
            RunCheck {
                id: CheckId::parse("tls.caa_compliant").unwrap(),
                status: Status::Pass,
            },
            RunCheck {
                id: CheckId::parse("dns.dnssec_valid").unwrap(),
                status: Status::Unmeasured,
            },
        ],
        blocks,
    }
}

#[test]
fn grade_wire_form_round_trips_for_every_variant() {
    let rows = [
        (Grade::APlus, "A+"),
        (Grade::A, "A"),
        (Grade::B, "B"),
        (Grade::C, "C"),
        (Grade::D, "D"),
        (Grade::F, "F"),
        (Grade::Incomplete, "incomplete"),
    ];
    for (grade, wire) in rows {
        assert_eq!(to_value(grade).unwrap(), json!(wire));
        let back: Grade = serde_json::from_value(json!(wire)).unwrap();
        assert_eq!(back, grade);
    }
}

#[test]
fn run_round_trips_through_json_unchanged() {
    let run = run_with(vec![]);
    let wire = serde_json::to_string(&run).unwrap();
    let back: Run = serde_json::from_str(&wire).unwrap();
    assert_eq!(back, run);

    let v = to_value(&run).unwrap();
    assert_eq!(v["grade"], json!("A+"));
    assert_eq!(v["sections"][1]["outcome"], json!("timed_out"));
    assert_eq!(v["checks"][1]["status"], json!("unmeasured"));
}

#[test]
fn run_with_all_six_evidence_shapes_round_trips() {
    let addr: IpAddr = "192.0.2.1".parse().unwrap();
    let blocks = vec![
        block(
            "dns.a",
            Shape::Records(vec![Record {
                owner: text("example.com"),
                rtype: text("A"),
                ttl: Value::Duration(300),
                data: Value::Addr(addr),
            }]),
        ),
        block(
            "http.headers",
            Shape::Table {
                key: "name".into(),
                columns: vec!["name".into(), "value".into()],
                rows: vec![vec![text("server"), text("example")]],
            },
        ),
        block(
            "tls.chain@192.0.2.1:443",
            Shape::Chain(vec![
                vec![("subject".into(), text("example.com"))],
                vec![("subject".into(), text("Example CA"))],
            ]),
        ),
        block(
            "email.tree",
            Shape::Tree(Node {
                fields: vec![("name".into(), text("root"))],
                children: vec![Node {
                    fields: vec![("name".into(), text("leaf"))],
                    children: vec![],
                }],
            }),
        ),
        block(
            "dns.caa",
            Shape::Fields(vec![
                ("data".into(), text("letsencrypt.org")),
                ("critical".into(), Value::Bool(false)),
                ("count".into(), Value::Int(-1)),
                ("seen".into(), Value::Time("2026-12-01T00:00:00Z".into())),
            ]),
        ),
        block("ip.note", Shape::Text("documentation prefix".into())),
    ];
    let run = run_with(blocks);
    let wire = serde_json::to_string(&run).unwrap();
    let back: Run = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.blocks.len(), 6);
    assert_eq!(back, run);
}
