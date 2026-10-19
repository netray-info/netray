//! C23 (spec v2-admission, Phase 3): `history::score_run` is the full `Score` of a stored run;
//! its grade is `rescore`'s, its percentage the one scoring the same events gives.

use netray_engine::history::{rescore, score_run};
use netray_engine::profile::Profiles;
use netray_engine::{SectionEvent, SectionOutcome, score};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};

fn dns_events(dnssec: Status, caa: Status) -> Vec<SectionEvent> {
    let check = |id: &str, status| CheckResult {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    };
    vec![SectionEvent {
        protocol: Protocol::Dns,
        outcome: SectionOutcome::Measured {
            checks: vec![check("dns.dnssec", dnssec), check("dns.caa", caa)],
            evidence: vec![],
        },
    }]
}

/// The run an engine pass over `dns_events` stores: grade from the scored events.
fn stored_run(dnssec: Status, caa: Status) -> (Run, Vec<SectionEvent>) {
    let profile = Profiles::current();
    let events = dns_events(dnssec, caa);
    let run = Run {
        profile_version: profile.version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: 1,
        grade: score(profile, &events).grade,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![
            RunCheck {
                id: CheckId::parse("dns.dnssec").unwrap(),
                status: dnssec,
                findings: vec![],
                evidence: vec![],
            },
            RunCheck {
                id: CheckId::parse("dns.caa").unwrap(),
                status: caa,
                findings: vec![],
                evidence: vec![],
            },
        ],
        blocks: vec![],
    };
    (run, events)
}

#[test]
fn score_run_gives_the_runs_grade_and_the_percentage_of_its_events() {
    let profile = Profiles::current();
    // (dnssec, caa, literal percentage, literal grade): weights 5 and 5 in the dns section.
    let rows = [
        (Status::Pass, Status::Fail, 50.0, Grade::D),
        (Status::Pass, Status::Pass, 100.0, Grade::APlus),
    ];
    for (dnssec, caa, percentage, grade) in rows {
        let (run, events) = stored_run(dnssec, caa);
        let scored = score_run(&run, profile);

        assert_eq!(scored.grade, run.grade);
        assert_eq!(scored.grade, grade);
        assert_eq!(rescore(&run, profile), scored.grade);
        assert_eq!(scored.overall_percentage, percentage);
        assert_eq!(
            scored.overall_percentage,
            score(profile, &events).overall_percentage
        );
    }
}
