//! Spectrum runs: shape, tones and heights of the multi-run layout (V2 spec v2-spectrum,
//! Phase 2, requirements 10-12).
use std::collections::BTreeMap;

use netray_engine::catalogue;
use netray_engine::profile::{Profile, Profiles};
use netray_model::{CheckId, Protocol, Status};
use netray_web::spectrum::{Layout, LineTone, RunInput, SectionState, layout};

const W: u32 = 1056;
const EPS: f64 = 1e-9;
/// The smallest v3 height with every check measured: 2/103 x 35 / 10 (tls `san_quality` and kin).
const SMALLEST_ALL_PASS: f64 = 7.0 / 103.0;

fn near(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() < eps
}

fn id(s: &str) -> CheckId {
    CheckId::parse(s).expect("check id")
}

/// Every catalogue check with `Pass`, then `over` applied; `None` removes the check.
fn statuses(over: &[(&str, Option<Status>)]) -> BTreeMap<CheckId, Status> {
    let mut m: BTreeMap<CheckId, Status> = catalogue()
        .entries()
        .iter()
        .map(|e| (e.id.clone(), Status::Pass))
        .collect();
    for (k, v) in over {
        match v {
            Some(s) => {
                m.insert(id(k), *s);
            }
            None => {
                m.remove(&id(k));
            }
        }
    }
    m
}

fn run_with<'a>(
    statuses: BTreeMap<CheckId, Status>,
    sections: &[(Protocol, SectionState)],
    profile: &'a Profile,
) -> RunInput<'a> {
    RunInput {
        statuses,
        sections: sections.iter().copied().collect(),
        profile,
    }
}

fn run(over: &[(&str, Option<Status>)]) -> RunInput<'static> {
    run_with(statuses(over), &[], Profiles::current())
}

fn lay(current: &RunInput, older: &[RunInput]) -> Layout {
    layout(current, older, catalogue(), W).expect("layout")
}

fn at(l: &Layout, check: &str) -> usize {
    l.lines
        .iter()
        .position(|x| x.id == id(check))
        .unwrap_or_else(|| panic!("no line {check}"))
}

/// The smallest height among the measured (non-muted) lines of run `r`.
fn smallest_measured(l: &Layout, r: usize) -> f64 {
    l.runs[r]
        .tones
        .iter()
        .zip(&l.runs[r].heights)
        .filter(|(t, _)| **t != LineTone::Muted)
        .map(|(_, h)| *h)
        .fold(f64::INFINITY, f64::min)
}

fn label_texts(l: &Layout, r: usize) -> Vec<&str> {
    l.runs[r].labels.iter().map(|x| x.text.as_str()).collect()
}

#[test]
fn r10_shape_lines_groups_runs_sections() {
    let cur = run_with(
        statuses(&[]),
        &[(Protocol::Tls, SectionState::NotApplicable)],
        Profiles::current(),
    );
    let o1 = run(&[("dns.caa", Some(Status::Fail))]);
    let o2 = run(&[("dns.caa", Some(Status::Warn))]);
    let l = lay(&cur, &[o1, o2]);

    assert_eq!(l.lines.len(), 54);
    let mut expected = Vec::new();
    for p in [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ] {
        expected.extend(
            catalogue()
                .entries()
                .iter()
                .filter(|e| e.protocol == p)
                .map(|e| e.id.clone()),
        );
    }
    let got: Vec<CheckId> = l.lines.iter().map(|x| x.id.clone()).collect();
    assert_eq!(got, expected);
    for x in &l.lines {
        assert_eq!(x.protocol, x.id.protocol());
    }

    assert_eq!(l.groups.len(), 5);
    let protos = [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ];
    let lens = [10, 21, 12, 10, 1];
    let mut first = 0;
    for (g, (p, n)) in l.groups.iter().zip(protos.iter().zip(lens)) {
        assert_eq!((g.first, g.len), (first, n));
        assert_eq!(l.lines[g.first].protocol, *p);
        first += n;
    }

    assert_eq!(l.runs.len(), 3);
    let caa = at(&l, "dns.caa");
    assert_eq!(l.runs[0].tones[caa], LineTone::Fixed); // pass after fail (R11)
    assert_eq!(l.runs[1].tones[caa], LineTone::Fail);
    assert_eq!(l.runs[2].tones[caa], LineTone::Warn);
    for r in &l.runs {
        assert_eq!(r.tones.len(), 54);
        assert_eq!(r.heights.len(), 54);
    }
    assert_eq!(l.runs[0].sections.len(), 5);
    assert_eq!(
        l.runs[0].sections[&Protocol::Tls],
        SectionState::NotApplicable
    );
    assert_eq!(l.runs[0].sections[&Protocol::Dns], SectionState::Measured);
    assert_eq!(l.runs[1].sections[&Protocol::Tls], SectionState::Measured);
}

#[test]
fn r11_tone_mapping() {
    let caa = |s: Option<Status>| run(&[("dns.caa", s)]);

    // current run alone: status -> tone
    let table = [
        (Some(Status::Pass), LineTone::Pass),
        (Some(Status::Warn), LineTone::Warn),
        (Some(Status::Fail), LineTone::Fail),
        (Some(Status::NotApplicable), LineTone::Muted),
        (Some(Status::NotTested), LineTone::Muted),
        (Some(Status::Unmeasured), LineTone::Muted),
        (None, LineTone::Muted),
    ];
    for (s, want) in table {
        let l = lay(&caa(s), &[]);
        assert_eq!(l.runs[0].tones[at(&l, "dns.caa")], want, "current {s:?}");
    }

    // (older, current) -> current tone: Fixed only for a pass after a warn or fail
    let pairs = [
        (Some(Status::Fail), Some(Status::Pass), LineTone::Fixed),
        (Some(Status::Warn), Some(Status::Pass), LineTone::Fixed),
        (Some(Status::Pass), Some(Status::Pass), LineTone::Pass),
        (
            Some(Status::NotApplicable),
            Some(Status::Pass),
            LineTone::Pass,
        ),
        (Some(Status::NotTested), Some(Status::Pass), LineTone::Pass),
        (Some(Status::Unmeasured), Some(Status::Pass), LineTone::Pass),
        (None, Some(Status::Pass), LineTone::Pass),
        (Some(Status::Fail), Some(Status::Warn), LineTone::Warn),
        (Some(Status::Fail), Some(Status::Fail), LineTone::Fail),
        (
            Some(Status::Fail),
            Some(Status::NotApplicable),
            LineTone::Muted,
        ),
    ];
    for (old, cur, want) in pairs {
        let l = lay(&caa(cur), &[caa(old)]);
        assert_eq!(
            l.runs[0].tones[at(&l, "dns.caa")],
            want,
            "older {old:?}, current {cur:?}"
        );
    }

    // fixed against the next older run only
    let l = lay(
        &caa(Some(Status::Pass)),
        &[caa(Some(Status::Pass)), caa(Some(Status::Fail))],
    );
    assert_eq!(l.runs[0].tones[at(&l, "dns.caa")], LineTone::Pass);

    // the oldest run is never fixed
    let l = lay(&caa(Some(Status::Fail)), &[caa(Some(Status::Pass))]);
    assert!(!l.runs[1].tones.contains(&LineTone::Fixed));

    // a check the profile does not weight is muted
    let mut p = Profiles::current().clone();
    p.sections
        .get_mut(&Protocol::Dns)
        .unwrap()
        .checks
        .remove(&id("dns.caa"));
    let cur = run_with(statuses(&[]), &[], &p);
    let l = lay(&cur, &[]);
    assert_eq!(l.runs[0].tones[at(&l, "dns.caa")], LineTone::Muted);

    // a section that is not measured mutes its lines
    let cur = run_with(
        statuses(&[]),
        &[
            (Protocol::Tls, SectionState::NotApplicable),
            (Protocol::Dns, SectionState::Unmeasured),
        ],
        Profiles::current(),
    );
    let l = lay(&cur, &[]);
    for (i, x) in l.lines.iter().enumerate() {
        let want = match x.protocol {
            Protocol::Tls | Protocol::Dns => LineTone::Muted,
            _ => LineTone::Pass,
        };
        assert_eq!(l.runs[0].tones[i], want, "{}", x.id);
    }
}

#[test]
fn r12_heights_normalised_muted_smallest() {
    let l = lay(&run(&[("dns.ns_lame", Some(Status::NotApplicable))]), &[]);
    let h = &l.runs[0].heights;
    // ip.reputation: e = 10 is the largest, so 1.0
    assert!(near(h[at(&l, "ip.reputation")], 1.0, EPS));
    // dns.dnssec: 5 / 30 x 20 = 10/3, normalised 1/3
    assert!(near(h[at(&l, "dns.dnssec")], 1.0 / 3.0, EPS));
    // dns.ttl: 2 / 30 x 20 / 10 = 2/15
    assert!(near(h[at(&l, "dns.ttl")], 2.0 / 15.0, EPS));
    // the muted line takes the smallest measured height
    assert!(near(h[at(&l, "dns.ns_lame")], SMALLEST_ALL_PASS, EPS));
    assert!(near(smallest_measured(&l, 0), SMALLEST_ALL_PASS, EPS));
    assert!(h.iter().all(|v| *v > 0.0 && *v <= 1.0));

    // a run with no measured line draws every line at 1.0
    let l = lay(&run_with(BTreeMap::new(), &[], Profiles::current()), &[]);
    assert!(l.runs[0].tones.iter().all(|t| *t == LineTone::Muted));
    assert!(l.runs[0].heights.iter().all(|v| near(*v, 1.0, EPS)));
}

#[test]
fn c6_real_catalogue_boundaries() {
    let l = lay(&run(&[]), &[]);
    assert_eq!(l.lines[0].id, id("dns.dnssec"));
    assert_eq!(l.lines[53].id, id("ip.reputation"));
    let pitch = 1056.0 / 54.0;
    assert!(near(l.pitch, pitch, EPS));
    let want = [10.0, 31.0, 43.0, 53.0];
    assert_eq!(l.boundaries.len(), 4);
    for (b, n) in l.boundaries.iter().zip(want) {
        assert!(near(*b, n * pitch, EPS), "boundary {b} vs line {n}");
    }
}

#[test]
fn c7_labels_for_warn_fail_and_fixed() {
    let cur = run(&[
        ("dns.caa", Some(Status::Warn)),
        ("tls.not_expired", Some(Status::Fail)),
    ]);
    let old = run(&[("http.hsts", Some(Status::Fail))]);
    let l = lay(&cur, &[old]);
    assert_eq!(label_texts(&l, 0), ["CAA", "Expired", "HSTS"]);
    assert_eq!(l.runs[0].tones[at(&l, "http.hsts")], LineTone::Fixed);
    assert_eq!(l.runs[0].tones[at(&l, "dns.caa")], LineTone::Warn);
    assert_eq!(l.runs[0].tones[at(&l, "tls.not_expired")], LineTone::Fail);
}

#[test]
fn c8_not_applicable_after_warn_is_muted_not_fixed() {
    let cur = run(&[("dns.caa", Some(Status::NotApplicable))]);
    let old = run(&[("dns.caa", Some(Status::Warn))]);
    let l = lay(&cur, &[old]);
    assert_eq!(l.runs[0].tones[at(&l, "dns.caa")], LineTone::Muted);
    assert!(!l.runs[0].tones.contains(&LineTone::Fixed));
    assert!(l.runs[0].labels.is_empty());
}

#[test]
fn c9_three_runs_oldest_unfixed_x_shared() {
    let r0 = run(&[]);
    let r1 = run(&[
        ("http.hsts", Some(Status::Fail)),
        ("dns.caa", Some(Status::Pass)),
    ]);
    let r2 = run(&[("dns.caa", Some(Status::Fail))]);
    let l = lay(&r0, &[r1, r2]);
    assert_eq!(l.runs.len(), 3);
    assert_eq!(l.runs[0].tones[at(&l, "http.hsts")], LineTone::Fixed);
    assert_eq!(l.runs[1].tones[at(&l, "dns.caa")], LineTone::Fixed);
    assert!(!l.runs[2].tones.contains(&LineTone::Fixed));
    assert_eq!(l.runs[2].tones[at(&l, "dns.caa")], LineTone::Fail);
    for (i, x) in l.lines.iter().enumerate() {
        assert!(near(x.x, (i as f64 + 0.5) * l.pitch, EPS), "x of line {i}");
    }
    for r in &l.runs {
        assert!(!r.labels.is_empty());
        for lab in &r.labels {
            assert!(near(lab.leader, l.lines[lab.line].x, EPS));
        }
    }
}

#[test]
fn c10_missing_check_is_muted_at_smallest_height() {
    let l = lay(&run(&[("email.bimi", None)]), &[]);
    let i = at(&l, "email.bimi");
    assert_eq!(l.runs[0].tones[i], LineTone::Muted);
    let small = smallest_measured(&l, 0);
    assert!(small > 0.0);
    assert!(near(l.runs[0].heights[i], small, EPS));
    assert!(near(small, SMALLEST_ALL_PASS, EPS));
}

#[test]
fn c11_all_pass_v3_heights() {
    let l = lay(&run(&[]), &[]);
    let h = &l.runs[0].heights;
    let rep = h[at(&l, "ip.reputation")];
    let reach = h[at(&l, "tls.tls_reachable")];
    let csp = h[at(&l, "http.csp")];
    assert!(near(rep, 1.0, EPS));
    assert!(near(reach, 10.0 / 103.0 * 35.0 / 10.0, EPS));
    assert!(near(csp, 1.0 / 23.0 * 20.0 / 10.0, EPS));
    // printed roundings: 1.00, 0.34, 0.09 (0.087)
    assert!(near(reach, 0.34, 0.005));
    assert!(near(csp, 0.087, 0.005));
}

#[test]
fn c12_lone_measured_check_in_section() {
    let mut over: Vec<(String, Option<Status>)> = catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == Protocol::Tls && e.id != id("tls.tls_reachable"))
        .map(|e| (e.id.to_string(), Some(Status::NotApplicable)))
        .collect();
    over.push(("tls.tls_reachable".into(), Some(Status::Fail)));
    let over: Vec<(&str, Option<Status>)> = over.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let l = lay(&run(&over), &[]);
    let h = &l.runs[0].heights;
    // e = 10/10 x 35 = 35, the run's largest
    assert!(near(h[at(&l, "tls.tls_reachable")], 1.0, EPS));
    assert_eq!(l.runs[0].tones[at(&l, "tls.tls_reachable")], LineTone::Fail);
    assert!(near(h[at(&l, "ip.reputation")], 10.0 / 35.0, EPS));
    assert!(near(h[at(&l, "ip.reputation")], 0.286, 0.0005));
}

#[test]
fn c13_not_applicable_dmarc_is_muted_small() {
    let l = lay(&run(&[("email.dmarc", Some(Status::NotApplicable))]), &[]);
    let i = at(&l, "email.dmarc");
    assert_eq!(l.runs[0].tones[i], LineTone::Muted);
    let small = smallest_measured(&l, 0);
    assert!(near(l.runs[0].heights[i], small, EPS));
    // measured, dmarc would stand at 4/22 x 15 / 10 = 0.2727
    assert!(l.runs[0].heights[i] < 0.27);
    assert!(near(small, SMALLEST_ALL_PASS, EPS));
}

#[test]
fn c14_all_email_not_applicable() {
    let over: Vec<(String, Option<Status>)> = catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == Protocol::Email)
        .map(|e| (e.id.to_string(), Some(Status::NotApplicable)))
        .collect();
    let over: Vec<(&str, Option<Status>)> = over.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let l = lay(&run(&over), &[]);
    let small = smallest_measured(&l, 0);
    assert!(near(small, SMALLEST_ALL_PASS, EPS));
    for (i, x) in l.lines.iter().enumerate() {
        if x.protocol == Protocol::Email {
            assert_eq!(l.runs[0].tones[i], LineTone::Muted, "{}", x.id);
            assert!(near(l.runs[0].heights[i], small, EPS), "{}", x.id);
        } else {
            assert_eq!(l.runs[0].tones[i], LineTone::Pass, "{}", x.id);
            assert!(l.runs[0].heights[i] >= small - EPS);
        }
    }
}

#[test]
fn c15_older_profile_changes_heights_not_x() {
    let mut doubled = Profiles::current().clone();
    doubled.sections.get_mut(&Protocol::Dns).unwrap().weight *= 2;
    let cur = run(&[]);
    let old = run_with(statuses(&[]), &[], &doubled);
    let l = lay(&cur, &[old]);
    let i = at(&l, "dns.dnssec");
    // current: 5/35 x 20 / 10 = 2/7; older: 5/35 x 40 / 10 = 4/7
    assert!(near(l.runs[0].heights[i], 2.0 / 7.0, EPS));
    assert!(near(l.runs[1].heights[i], 4.0 / 7.0, EPS));
    // x is a property of the line, shared by both runs
    for (k, x) in l.lines.iter().enumerate() {
        assert!(near(x.x, (k as f64 + 0.5) * l.pitch, EPS));
    }
}

#[test]
fn c16_check_unweighted_in_older_profile() {
    let mut no_ttl = Profiles::current().clone();
    no_ttl
        .sections
        .get_mut(&Protocol::Dns)
        .unwrap()
        .checks
        .remove(&id("dns.ttl"));

    // dns.ttl passes in the older run but is not weighted there
    let cur = run(&[]);
    let old = run_with(statuses(&[]), &[], &no_ttl);
    let l = lay(&cur, &[old]);
    let i = at(&l, "dns.ttl");
    assert_eq!(l.runs[1].tones[i], LineTone::Muted);
    assert!(near(l.runs[1].heights[i], smallest_measured(&l, 1), EPS));
    assert!(l.runs[1].labels.iter().all(|x| x.line != i));
    assert_eq!(l.runs[0].tones[i], LineTone::Pass);

    // dns.ttl fails in the older run, unweighted there: no Fixed for the current pass
    let old = run_with(statuses(&[("dns.ttl", Some(Status::Fail))]), &[], &no_ttl);
    let l = lay(&cur, &[old]);
    assert_eq!(l.runs[1].tones[i], LineTone::Muted);
    assert_eq!(l.runs[0].tones[i], LineTone::Pass);
    assert!(!l.runs[0].tones.contains(&LineTone::Fixed));
}

/// Requirement 12: a section the grade leaves unscored (a weighted check unmeasured) mutes every
/// line in it and reports `Unmeasured`, so its answered lines draw no inflated height.
#[test]
fn r12_unmeasured_check_unscores_its_section() {
    // HTTPS probe unanswered: the response checks unmeasured, the port-80 redirect passes
    let unanswered: Vec<(String, Option<Status>)> = catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == Protocol::Http && e.id.to_string() != "http.https_redirect")
        .map(|e| (e.id.to_string(), Some(Status::Unmeasured)))
        .collect();
    let over: Vec<(&str, Option<Status>)> =
        unanswered.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let cur = run(&over);
    let l = lay(&cur, &[]);
    let r = &l.runs[0];
    assert_eq!(r.sections[&Protocol::Http], SectionState::Unmeasured);
    let redirect = at(&l, "http.https_redirect");
    assert_eq!(r.tones[redirect], LineTone::Muted);
    let smallest = smallest_measured(&l, 0);
    for (i, line) in l.lines.iter().enumerate() {
        if line.protocol == Protocol::Http {
            assert_eq!(r.tones[i], LineTone::Muted, "{}", line.id);
            assert!(near(r.heights[i], smallest, 1e-9), "{}", line.id);
        }
    }
    // the other sections stay scored: ip.reputation keeps full height
    assert!(near(r.heights[at(&l, "ip.reputation")], 1.0, 1e-9));
    assert_eq!(r.sections[&Protocol::Dns], SectionState::Measured);

    // one unmeasured email check (a refused DNSBL zone) unscores email the same way
    let cur = run(&[("email.dnsbl", Some(Status::Unmeasured))]);
    let l = lay(&cur, &[]);
    assert_eq!(
        l.runs[0].sections[&Protocol::Email],
        SectionState::Unmeasured
    );
    assert_eq!(l.runs[0].tones[at(&l, "email.dmarc")], LineTone::Muted);
}
