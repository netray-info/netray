//! C12: a measured hard fail gives grade F even when the run is incomplete. `complete` stays
//! false when a section is incomplete. Without a hard fail, an incomplete run stays Incomplete.

use netray_engine::profile::Profiles;
use netray_engine::{SectionEvent, SectionOutcome, score};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};

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
            evidence: vec![],
        },
    }
}

fn tls_not_expired(status: Status) -> SectionEvent {
    measured(Protocol::Tls, vec![check("tls.not_expired", status)])
}

#[test]
fn hard_fail_outranks_incomplete() {
    // (name, events, expected grade, expects hard fail named)
    let cases: Vec<(&str, Vec<SectionEvent>, Grade, bool)> = vec![
        (
            "hard fail + unmeasured weighted check",
            vec![
                tls_not_expired(Status::Fail),
                measured(Protocol::Dns, vec![check("dns.dnssec", Status::Unmeasured)]),
            ],
            Grade::F,
            true,
        ),
        (
            "hard fail + timed-out section",
            vec![
                tls_not_expired(Status::Fail),
                SectionEvent {
                    protocol: Protocol::Dns,
                    outcome: SectionOutcome::TimedOut,
                },
            ],
            Grade::F,
            true,
        ),
        (
            "hard fail + incomplete section",
            vec![
                tls_not_expired(Status::Fail),
                SectionEvent {
                    protocol: Protocol::Dns,
                    outcome: SectionOutcome::Incomplete {
                        reason: "resolve failed".into(),
                    },
                },
            ],
            Grade::F,
            true,
        ),
        (
            "no hard fail + unmeasured weighted check",
            vec![
                tls_not_expired(Status::Pass),
                measured(Protocol::Dns, vec![check("dns.dnssec", Status::Unmeasured)]),
            ],
            Grade::Incomplete,
            false,
        ),
    ];
    for (name, events, grade, hard) in cases {
        let s = score(Profiles::get(3).unwrap(), &events);
        assert_eq!(s.grade, grade, "{name}: grade");
        assert!(!s.complete, "{name}: complete");
        assert_eq!(!s.hard_fail_checks.is_empty(), hard, "{name}: hard fail");
        if hard {
            assert_eq!(
                s.hard_fail_checks
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>(),
                vec!["tls.not_expired".to_string()],
                "{name}: hard_fail_checks"
            );
        }
    }
}
