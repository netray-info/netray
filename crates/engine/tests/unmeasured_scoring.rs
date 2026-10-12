//! C1, C4, C5, C6: a weighted `Unmeasured` check makes its section incomplete, so the run gets
//! no letter. `NotApplicable` and `NotTested` stay excluded from possible and keep the section
//! complete. An incomplete section is absent from `Score::sections`.

use netray_engine::profile::Profile;
use netray_engine::{score, SectionEvent, SectionOutcome};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};

const PROFILE: &str = r#"
[meta]
name = "test"
version = 3

[sections.dns]
weight = 100
hard_fail = []

[sections.dns.checks]
spf = 5
dmarc = 3

[thresholds]
"A+" = 97
"A" = 90
"B" = 75
"C" = 60
"D" = 40
"F" = 0
"#;

fn dns_profile() -> Profile {
    Profile::from_toml(PROFILE).expect("test profile parses")
}

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
        },
    }
}

#[test]
fn unmeasured_weighted_check_makes_section_incomplete() {
    // (dmarc status, section complete, run has a letter)
    let cases = [
        (Status::Unmeasured, false, false),  // C1, C4
        (Status::NotApplicable, true, true), // C5
        (Status::NotTested, true, true),     // C6
    ];
    for (dmarc, section_complete, has_letter) in cases {
        let s = score(
            &dns_profile(),
            &[measured(
                Protocol::Dns,
                vec![check("dns.spf", Status::Pass), check("dns.dmarc", dmarc)],
            )],
        );
        assert_eq!(
            s.sections.contains_key(&Protocol::Dns),
            section_complete,
            "section scored: {dmarc:?}"
        );
        assert_eq!(s.complete, section_complete, "complete: {dmarc:?}");
        assert_eq!(
            s.grade != Grade::Incomplete,
            has_letter,
            "letter: {dmarc:?} -> {:?}",
            s.grade
        );
        if section_complete {
            // Excluded statuses drop out of possible: only spf's weight remains.
            assert_eq!(s.sections[&Protocol::Dns].possible, 5, "{dmarc:?}");
        }
    }
}
