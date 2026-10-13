//! V1 scoring cases over `netray_engine::score`: every failing hard-fail check is reported, and
//! the grade thresholds hold at their boundaries.

use netray_engine::profile::Profile;
use netray_engine::{SectionEvent, SectionOutcome, score};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};

const THRESHOLDS: &str = r#"
[thresholds]
"A+" = 97
"A" = 90
"B" = 75
"C" = 60
"D" = 40
"F" = 0
"#;

fn check(id: &str, status: Status) -> CheckResult {
    CheckResult {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    }
}

fn measured(protocol: Protocol, checks: Vec<CheckResult>) -> SectionEvent {
    SectionEvent {
        protocol,
        outcome: SectionOutcome::Measured {
            checks,
            presentation: serde_json::Value::Null,
            evidence: vec![],
        },
    }
}

#[test]
fn two_failing_hard_fail_checks_are_both_reported() {
    let p = Profile::from_toml(&format!(
        "[meta]\nname = \"test\"\nversion = 3\n\n\
         [sections.tls]\nweight = 100\nhard_fail = [\"chain_trusted\", \"not_expired\"]\n\n\
         [sections.tls.checks]\nchain_trusted = 10\nnot_expired = 10\nhostname_match = 10\n{THRESHOLDS}"
    ))
    .unwrap();
    let s = score(
        &p,
        &[measured(
            Protocol::Tls,
            vec![
                check("tls.chain_trusted", Status::Fail),
                check("tls.not_expired", Status::Fail),
                check("tls.hostname_match", Status::Pass),
            ],
        )],
    );
    assert_eq!(s.grade, Grade::F);
    assert_eq!(
        s.hard_fail_checks,
        vec![
            CheckId::parse("tls.chain_trusted").unwrap(),
            CheckId::parse("tls.not_expired").unwrap(),
        ]
    );
}

/// A one-section profile whose percentage is `pass_weight / 10`: `pass` passes, `fail` fails.
fn grade_at(pass_weight: u32) -> (f64, Grade) {
    let fail_weight = 1000 - pass_weight;
    let mut toml = String::from(
        "[meta]\nname = \"test\"\nversion = 3\n\n[sections.dns]\nweight = 100\n\n[sections.dns.checks]\n",
    );
    let mut checks = Vec::new();
    if pass_weight > 0 {
        toml.push_str(&format!("pass = {pass_weight}\n"));
        checks.push(check("dns.pass", Status::Pass));
    }
    if fail_weight > 0 {
        toml.push_str(&format!("fail = {fail_weight}\n"));
        checks.push(check("dns.fail", Status::Fail));
    }
    toml.push_str(THRESHOLDS);
    let p = Profile::from_toml(&toml).unwrap();
    let s = score(&p, &[measured(Protocol::Dns, checks)]);
    (s.overall_percentage, s.grade)
}

#[test]
fn threshold_boundaries() {
    for (pass_weight, expected) in [
        (900, Grade::A),
        (899, Grade::B),
        (750, Grade::B),
        (749, Grade::C),
        (600, Grade::C),
        (599, Grade::D),
        (0, Grade::F),
    ] {
        let (pct, grade) = grade_at(pass_weight);
        assert_eq!(grade, expected, "{pct}% must grade {expected:?}");
    }
}
