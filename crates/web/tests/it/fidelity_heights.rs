//! Line heights of the spectrum (V2 spec v2-result-fidelity, Phase 2, requirement 14, criteria
//! C1, C7, C8): a measured line is 0.35 + 0.65 x w / w_max high.
use std::collections::BTreeMap;

use netray_engine::catalogue;
use netray_engine::profile::Profiles;
use netray_model::{CheckId, Protocol, Status};
use netray_web::spectrum::{Layout, LineTone, RunInput, SectionState, layout};

const W: u32 = 1056;

fn id(s: &str) -> CheckId {
    CheckId::parse(s).expect("check id")
}

fn run(sections: &[(Protocol, SectionState)]) -> RunInput<'static> {
    let statuses: BTreeMap<CheckId, Status> = catalogue()
        .entries()
        .iter()
        .map(|e| (e.id.clone(), Status::Pass))
        .collect();
    RunInput {
        statuses,
        sections: sections.iter().copied().collect(),
        profile: Profiles::current(),
    }
}

fn lay(r: &RunInput) -> Layout {
    layout(r, &[], catalogue(), W).expect("layout")
}

fn height(l: &Layout, check: &str) -> f64 {
    let i = l
        .lines
        .iter()
        .position(|x| x.id == id(check))
        .unwrap_or_else(|| panic!("no line {check}"));
    l.runs[0].heights[i]
}

/// Effective weights of the current profile with every check measured (section weight scaled
/// share): ip.reputation 10, dns.dnssec 20/7, dns.ttl 8/7.
#[test]
fn c7_height_is_floor_plus_scaled_weight_share() {
    let l = lay(&run(&[]));
    let want = [
        ("ip.reputation", 1.0),
        ("dns.dnssec", 0.35 + 0.65 * 2.0 / 7.0),
        ("dns.ttl", 0.35 + 0.65 * 4.0 / 35.0),
    ];
    for (check, h) in want {
        let got = height(&l, check);
        assert!((got - h).abs() < 1e-9, "{check}: {got} vs {h}");
    }
    // on a 92 px plate the 10 : 5 : 2 example reads 92, 62.1, 44.2
    for (w, px) in [(10.0, 92.0), (5.0, 62.1), (2.0, 44.2)] {
        let got = (0.35 + 0.65 * w / 10.0) * 92.0;
        assert!((got - px as f64).abs() < 0.1, "w {w}: {got} vs {px}");
    }
    let l_min = l.runs[0]
        .heights
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    assert!(l_min >= 0.35, "lightest line {l_min}");
}

#[test]
fn c8_unmeasured_section_is_muted_and_leaves_w_max() {
    let l = lay(&run(&[(Protocol::Ip, SectionState::Unmeasured)]));
    let r = &l.runs[0];
    let mut top: f64 = 0.0;
    let mut low = f64::INFINITY;
    for (line, (tone, h)) in l.lines.iter().zip(r.tones.iter().zip(&r.heights)) {
        if line.protocol == Protocol::Ip {
            assert_eq!(*tone, LineTone::Muted, "{}", line.id);
        } else {
            assert_ne!(*tone, LineTone::Muted, "{}", line.id);
            top = top.max(*h);
            low = low.min(*h);
        }
    }
    assert!((top - 1.0).abs() < 1e-9, "heaviest measured line {top}");
    assert!(low >= 0.35, "lightest measured line {low}");
}
