//! Contract tests for the shared check model (V2 SDD).
use netray_model::{CheckId, CheckResult, FixOwner, Grade, Protocol, Severity, Status};
use serde_json::{json, to_value};

#[test]
fn check_id_parses_protocol_and_name() {
    let id = CheckId::parse("tls.chain_trusted").expect("valid id");
    assert_eq!(id.protocol(), Protocol::Tls);
    assert_eq!(id.name(), "chain_trusted");
}

#[test]
fn check_id_refuses_malformed_input() {
    for bad in [
        "chain_trusted",
        "smtp.x",
        "tls.",
        "tls.a.b",
        "tls.Chain",
        "tls.chain trusted",
        "tls.1chain",
    ] {
        assert!(CheckId::parse(bad).is_err(), "{bad:?} must be refused");
    }
}

#[test]
fn check_id_accepts_snake_case_names() {
    for good in ["tls.tls_reachable", "dns.caa", "email.mta_sts", "http.x2"] {
        assert!(CheckId::parse(good).is_ok(), "{good:?} must parse");
    }
}

#[test]
fn status_serialises_snake_case() {
    let rows = [
        (Status::Pass, "pass"),
        (Status::Warn, "warn"),
        (Status::Fail, "fail"),
        (Status::NotApplicable, "not_applicable"),
        (Status::NotTested, "not_tested"),
        (Status::Unmeasured, "unmeasured"),
    ];
    for (status, expected) in rows {
        assert_eq!(to_value(status).unwrap(), json!(expected));
    }
}

#[test]
fn grade_serialises_as_letter_or_incomplete() {
    let rows = [
        (Grade::APlus, "A+"),
        (Grade::A, "A"),
        (Grade::B, "B"),
        (Grade::C, "C"),
        (Grade::D, "D"),
        (Grade::F, "F"),
        (Grade::Incomplete, "incomplete"),
    ];
    for (grade, expected) in rows {
        assert_eq!(to_value(grade).unwrap(), json!(expected));
    }
}

#[test]
fn severity_fix_owner_protocol_serialise_snake_case() {
    let severities = [
        (Severity::Critical, "critical"),
        (Severity::High, "high"),
        (Severity::Medium, "medium"),
        (Severity::Low, "low"),
    ];
    for (v, expected) in severities {
        assert_eq!(to_value(v).unwrap(), json!(expected));
    }
    let owners = [
        (FixOwner::DnsProvider, "dns_provider"),
        (FixOwner::Registrar, "registrar"),
        (FixOwner::CertificateProvider, "certificate_provider"),
        (FixOwner::WebServer, "web_server"),
        (FixOwner::MailProvider, "mail_provider"),
        (FixOwner::Hosting, "hosting"),
    ];
    for (v, expected) in owners {
        assert_eq!(to_value(v).unwrap(), json!(expected));
    }
    let protocols = [
        (Protocol::Dns, "dns"),
        (Protocol::Tls, "tls"),
        (Protocol::Http, "http"),
        (Protocol::Email, "email"),
        (Protocol::Ip, "ip"),
    ];
    for (v, expected) in protocols {
        assert_eq!(to_value(v).unwrap(), json!(expected));
    }
}

#[test]
fn check_result_serialises_all_fields() {
    let result = CheckResult {
        id: CheckId::parse("dns.dnssec_valid").unwrap(),
        status: Status::Warn,
        findings: vec!["DS record missing".to_string()],
        evidence: vec!["example.com. IN DS".to_string()],
    };
    let value = to_value(&result).unwrap();
    assert_eq!(value["id"], json!("dns.dnssec_valid"));
    assert_eq!(value["status"], json!("warn"));
    assert_eq!(value["findings"], json!(["DS record missing"]));
    assert_eq!(value["evidence"], json!(["example.com. IN DS"]));
}
