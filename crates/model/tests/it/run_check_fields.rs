//! v2-api R4 / C33: `RunCheck` carries `findings` and `evidence`, both omitted when empty.
use netray_model::evidence::BlockId;
use netray_model::run::{Run, RunCheck};
use netray_model::{CheckId, Status};
use serde_json::{json, to_value};

#[test]
fn run_without_findings_or_evidence_parses_empty_and_round_trips_byte_identical() {
    let text = r#"{"profile_version":3,"engine_version":"0.26.0","evidence_schema_version":1,"grade":"A","complete":true,"sections":[],"checks":[{"id":"dns.caa","status":"pass"}],"blocks":[]}"#;
    let run: Run = serde_json::from_str(text).expect("parses without the new keys");
    assert!(run.checks[0].findings.is_empty());
    assert!(run.checks[0].evidence.is_empty());
    let again = serde_json::to_string(&run).unwrap();
    let reparsed: serde_json::Value = serde_json::from_str(&again).unwrap();
    assert!(reparsed["checks"][0].get("findings").is_none());
    assert!(reparsed["checks"][0].get("evidence").is_none());
    let original: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(reparsed, original);
}

#[test]
fn run_check_serialises_findings_and_evidence_only_when_non_empty() {
    let full = RunCheck {
        id: CheckId::parse("tls.caa_compliant").unwrap(),
        status: Status::Warn,
        findings: vec!["x".into()],
        evidence: vec![BlockId::parse("dns.caa").unwrap()],
    };
    assert_eq!(
        to_value(&full).unwrap(),
        json!({"id":"tls.caa_compliant","status":"warn","findings":["x"],"evidence":["dns.caa"]})
    );
    let empty = RunCheck {
        id: CheckId::parse("tls.caa_compliant").unwrap(),
        status: Status::Warn,
        findings: vec![],
        evidence: vec![],
    };
    assert_eq!(
        to_value(&empty).unwrap(),
        json!({"id":"tls.caa_compliant","status":"warn"})
    );
}
