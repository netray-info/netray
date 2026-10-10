//! C3, C4: `netray_engine::score` carries V1's scoring rules over the engine's section events:
//! Warn earns exactly half (f64), excluded statuses, possible 0 / Incomplete / TimedOut ->
//! incomplete, N/A sections listed, hard fail -> F, weighted overall, threshold boundaries.

use netray_engine::profile::Profile;
use netray_engine::scoring::SectionScore;
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

/// `(protocol, section weight, hard_fail names, [(check name, weight)])`.
type SectionSpec<'a> = (&'a str, u32, &'a [&'a str], &'a [(&'a str, u32)]);

/// A profile from its section specs.
fn profile(sections: &[SectionSpec]) -> Profile {
    let mut toml = String::from("[meta]\nname = \"test\"\nversion = 3\n");
    for (proto, weight, hard_fail, checks) in sections {
        let hf: Vec<String> = hard_fail.iter().map(|n| format!("\"{n}\"")).collect();
        toml.push_str(&format!(
            "\n[sections.{proto}]\nweight = {weight}\nhard_fail = [{}]\n\n[sections.{proto}.checks]\n",
            hf.join(", ")
        ));
        for (name, w) in *checks {
            toml.push_str(&format!("{name} = {w}\n"));
        }
    }
    toml.push_str(THRESHOLDS);
    Profile::from_toml(&toml).expect("test profile parses")
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

fn outcome(protocol: Protocol, outcome: SectionOutcome) -> SectionEvent {
    SectionEvent { protocol, outcome }
}

fn dns_profile() -> Profile {
    profile(&[("dns", 100, &[], &[("spf", 5), ("dmarc", 3)])])
}

#[test]
fn warn_earns_half_of_the_weight() {
    let p = profile(&[("dns", 100, &[], &[("spf", 1)])]);
    let s = score(
        &p,
        &[measured(
            Protocol::Dns,
            vec![check("dns.spf", Status::Warn)],
        )],
    );
    assert_eq!(
        s.sections[&Protocol::Dns],
        SectionScore {
            earned: 0.5,
            possible: 1,
            percentage: 50.0
        }
    );

    let p = profile(&[("dns", 100, &[], &[("spf", 5)])]);
    let s = score(
        &p,
        &[measured(
            Protocol::Dns,
            vec![check("dns.spf", Status::Warn)],
        )],
    );
    assert_eq!(
        s.sections[&Protocol::Dns],
        SectionScore {
            earned: 2.5,
            possible: 5,
            percentage: 50.0
        }
    );
}

#[test]
fn per_check_status_rules() {
    // (spf status, dmarc status) -> (earned, possible)
    let cases = [
        (Status::Pass, Status::Pass, 8.0, 8),
        (Status::Fail, Status::Fail, 0.0, 8),
        (Status::Pass, Status::NotApplicable, 5.0, 5),
        (Status::Pass, Status::NotTested, 5.0, 5),
        (Status::Pass, Status::Unmeasured, 5.0, 5),
        (Status::Warn, Status::Fail, 2.5, 8),
    ];
    for (spf, dmarc, earned, possible) in cases {
        let s = score(
            &dns_profile(),
            &[measured(
                Protocol::Dns,
                vec![check("dns.spf", spf), check("dns.dmarc", dmarc)],
            )],
        );
        let sec = &s.sections[&Protocol::Dns];
        assert_eq!(
            (sec.earned, sec.possible),
            (earned, possible),
            "{spf:?}/{dmarc:?}"
        );
        assert!(s.complete, "{spf:?}/{dmarc:?}");
    }
}

#[test]
fn check_without_profile_weight_is_ignored() {
    let s = score(
        &dns_profile(),
        &[measured(
            Protocol::Dns,
            vec![
                check("dns.spf", Status::Pass),
                check("dns.dmarc", Status::Pass),
                check("dns.unweighted", Status::Fail),
            ],
        )],
    );
    let sec = &s.sections[&Protocol::Dns];
    assert_eq!((sec.earned, sec.possible, sec.percentage), (8.0, 8, 100.0));
}

#[test]
fn every_check_excluded_is_incomplete() {
    let s = score(
        &dns_profile(),
        &[measured(
            Protocol::Dns,
            vec![
                check("dns.spf", Status::NotApplicable),
                check("dns.dmarc", Status::Unmeasured),
            ],
        )],
    );
    assert!(!s.complete);
    assert_eq!(s.grade, Grade::Incomplete);
}

#[test]
fn incomplete_and_timed_out_sections_make_the_grade_incomplete() {
    let two = profile(&[
        ("dns", 50, &[], &[("spf", 1)]),
        ("tls", 50, &[], &[("tls_reachable", 1)]),
    ]);
    for bad in [
        SectionOutcome::Incomplete {
            reason: "resolve failed".into(),
        },
        SectionOutcome::TimedOut,
    ] {
        let s = score(
            &two,
            &[
                measured(Protocol::Dns, vec![check("dns.spf", Status::Pass)]),
                outcome(Protocol::Tls, bad.clone()),
            ],
        );
        assert!(!s.complete, "{bad:?}");
        assert_eq!(s.grade, Grade::Incomplete, "{bad:?}");
    }
}

#[test]
fn not_applicable_section_is_excluded_and_listed() {
    let two = profile(&[
        ("dns", 50, &[], &[("spf", 1)]),
        ("email", 50, &[], &[("spf", 1)]),
    ]);
    let s = score(
        &two,
        &[
            measured(Protocol::Dns, vec![check("dns.spf", Status::Pass)]),
            outcome(
                Protocol::Email,
                SectionOutcome::NotApplicable {
                    reason: "no MX".into(),
                },
            ),
        ],
    );
    assert!(s.complete);
    assert_eq!(s.grade, Grade::APlus);
    assert_eq!(s.overall_percentage, 100.0);
    assert!(!s.sections.contains_key(&Protocol::Email));
    assert_eq!(s.not_applicable[&Protocol::Email], "no MX");
}

#[test]
fn profile_section_without_event_is_ignored() {
    let two = profile(&[
        ("dns", 50, &[], &[("spf", 1)]),
        ("tls", 50, &[], &[("tls_reachable", 1)]),
    ]);
    let s = score(
        &two,
        &[measured(
            Protocol::Dns,
            vec![check("dns.spf", Status::Pass)],
        )],
    );
    assert!(s.complete);
    assert_eq!(s.grade, Grade::APlus);
    assert!(!s.sections.contains_key(&Protocol::Tls));
}

#[test]
fn no_events_is_incomplete() {
    let s = score(&dns_profile(), &[]);
    assert!(!s.complete);
    assert_eq!(s.grade, Grade::Incomplete);
}

#[test]
fn hard_fail_forces_f_only_on_fail() {
    let p = profile(&[(
        "tls",
        100,
        &["chain_trusted"],
        &[("chain_trusted", 1), ("hsts", 99)],
    )]);
    let tls = |status| {
        vec![measured(
            Protocol::Tls,
            vec![
                check("tls.chain_trusted", status),
                check("tls.hsts", Status::Pass),
            ],
        )]
    };

    let failed = score(&p, &tls(Status::Fail));
    assert_eq!(failed.grade, Grade::F);
    assert_eq!(
        failed.hard_fail_checks,
        vec![CheckId::parse("tls.chain_trusted").unwrap()]
    );

    let warned = score(&p, &tls(Status::Warn));
    assert!(warned.hard_fail_checks.is_empty());
    assert_ne!(warned.grade, Grade::F);
}

#[test]
fn overall_is_the_section_weight_weighted_mean() {
    let two = profile(&[
        ("dns", 20, &[], &[("spf", 1)]),
        ("tls", 35, &[], &[("hsts", 2)]),
    ]);
    let s = score(
        &two,
        &[
            measured(Protocol::Dns, vec![check("dns.spf", Status::Pass)]),
            measured(Protocol::Tls, vec![check("tls.hsts", Status::Warn)]),
        ],
    );
    let expected = (100.0 * 20.0 + 50.0 * 35.0) / 55.0;
    assert!(
        (s.overall_percentage - expected).abs() < 1e-9,
        "{}",
        s.overall_percentage
    );
    assert!((s.overall_percentage - 68.18).abs() < 0.01);
}

#[test]
fn threshold_boundaries() {
    // (pass weight, fail weight, grade): percentage = pass / (pass + fail)
    let cases = [
        (97, 3, Grade::APlus), // 97.0
        (9699, 301, Grade::A), // 96.99
        (2, 3, Grade::D),      // 40.0
        (399, 601, Grade::F),  // 39.9
    ];
    for (pass, fail, grade) in cases {
        let p = profile(&[("dns", 100, &[], &[("spf", pass), ("dmarc", fail)])]);
        let s = score(
            &p,
            &[measured(
                Protocol::Dns,
                vec![
                    check("dns.spf", Status::Pass),
                    check("dns.dmarc", Status::Fail),
                ],
            )],
        );
        assert_eq!(s.grade, grade, "{pass}/{}", pass + fail);
    }
}
