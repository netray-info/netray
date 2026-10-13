//! Contract tests for the evidence vocabulary (spec v2-evidence, Phase 1, C4-C8).
use netray_model::evidence::{
    Block, BlockId, EVIDENCE_SCHEMA_VERSION, Register, Shape, ShapeKind, Value, ValueType,
    check_references, validate,
};
use netray_model::{CheckId, CheckResult, Status};
use serde_json::{json, to_value};
use std::net::IpAddr;

fn id(s: &str) -> BlockId {
    BlockId::parse(s).expect("valid block id")
}

fn fields_block(name: &str, key: &str, value: Value) -> Block {
    Block {
        id: id(name),
        title: "t".into(),
        shape: Shape::Fields(vec![(key.to_string(), value)]),
    }
}

#[test]
fn schema_version_is_one() {
    assert_eq!(EVIDENCE_SCHEMA_VERSION, 1);
}

#[test]
fn value_variants_round_trip_with_tagged_wire_form() {
    let addr: IpAddr = "192.0.2.1".parse().unwrap();
    let rows = [
        (Value::Text("x".into()), json!({"t": "text", "v": "x"})),
        (Value::Int(-7), json!({"t": "int", "v": -7})),
        (Value::Bool(true), json!({"t": "bool", "v": true})),
        (
            Value::Time("2026-12-01T00:00:00Z".into()),
            json!({"t": "time", "v": "2026-12-01T00:00:00Z"}),
        ),
        (Value::Duration(1500), json!({"t": "duration", "v": 1500})),
        (Value::Addr(addr), json!({"t": "addr", "v": "192.0.2.1"})),
    ];
    for (value, wire) in rows {
        assert_eq!(to_value(&value).unwrap(), wire, "wire form of {value:?}");
        let back: Value = serde_json::from_value(wire).unwrap();
        assert_eq!(back, value);
    }
}

#[test]
fn block_id_parses_module_name_qualifier() {
    let b = id("tls.chain@192.0.2.1:443");
    assert_eq!(b.module(), "tls");
    assert_eq!(b.name(), "chain");
    assert_eq!(b.qualifier(), Some("192.0.2.1:443"));

    let b = id("dns.caa");
    assert_eq!((b.module(), b.name(), b.qualifier()), ("dns", "caa", None));

    let b = id("tls.chain@[2001:db8::10]:443");
    assert_eq!(b.qualifier(), Some("[2001:db8::10]:443"));

    assert_eq!(id("tls.tlsa@443").qualifier(), Some("443"));
}

#[test]
fn block_id_round_trips_as_string() {
    for s in ["dns.caa", "tls.chain@[2001:db8::10]:443", "tls.tlsa@443"] {
        let b = id(s);
        assert_eq!(b.to_string(), s);
        assert_eq!(to_value(&b).unwrap(), json!(s));
        let back: BlockId = serde_json::from_value(json!(s)).unwrap();
        assert_eq!(back, b);
    }
}

#[test]
fn block_id_refuses_malformed_input() {
    for bad in ["tls..chain", "chain", "Tls.chain", "email.mta_sts.policy"] {
        assert!(BlockId::parse(bad).is_err(), "{bad:?} must be refused");
    }
}

const REGISTER: Register = Register {
    blocks: &[("dns.caa", ShapeKind::Fields)],
    keys: &[("dns.caa", "data", ValueType::Text)],
};

#[test]
fn validate_accepts_registered_key_and_type() {
    let blocks = [fields_block("dns.caa", "data", Value::Text("x".into()))];
    assert!(validate("dns", &REGISTER, &blocks).is_ok());
}

#[test]
fn validate_refuses_unregistered_key() {
    let blocks = [fields_block("dns.caa", "extra", Value::Text("x".into()))];
    let errs = validate("dns", &REGISTER, &blocks).expect_err("unregistered key");
    assert!(
        errs.iter().any(|e| e.to_string().contains("extra")),
        "{errs:?}"
    );
}

#[test]
fn validate_refuses_mistyped_value() {
    let blocks = [fields_block("dns.caa", "data", Value::Int(1))];
    let errs = validate("dns", &REGISTER, &blocks).expect_err("type mismatch");
    assert!(
        errs.iter().any(|e| e.to_string().contains("data")),
        "{errs:?}"
    );
}

#[test]
fn validate_refuses_foreign_module_block() {
    let blocks = [fields_block("dns.caa", "data", Value::Text("x".into()))];
    let errs = validate("tls", &REGISTER, &blocks).expect_err("foreign module");
    assert!(
        errs.iter().any(|e| e.to_string().contains("dns.caa")),
        "{errs:?}"
    );
}

#[test]
fn validate_refuses_unregistered_block_name() {
    let blocks = [Block {
        id: id("dns.nsec"),
        title: "t".into(),
        shape: Shape::Fields(vec![]),
    }];
    let errs = validate("dns", &REGISTER, &blocks).expect_err("unregistered block");
    assert!(
        errs.iter().any(|e| e.to_string().contains("dns.nsec")),
        "{errs:?}"
    );
}

fn caa_check() -> CheckResult {
    CheckResult {
        id: CheckId::parse("tls.caa_compliant").unwrap(),
        status: Status::Pass,
        findings: vec![],
        evidence: vec![id("dns.caa")],
    }
}

#[test]
fn check_references_accepts_block_of_another_module() {
    let blocks = [fields_block("dns.caa", "data", Value::Text("x".into()))];
    assert!(check_references(&[caa_check()], &blocks).is_ok());
}

#[test]
fn check_references_returns_dangling_ids() {
    let dangling = check_references(&[caa_check()], &[]).expect_err("dangling");
    assert_eq!(dangling, vec![id("dns.caa")]);
}
