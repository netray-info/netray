//! Property test of the spectrum layout over the real catalogue and profile (V2 spec
//! v2-spectrum, requirement 13, criteria C4, C17): requirement 8's claims (a)-(g) on every
//! run's labels, plus 57 lines, heights, shared x and the Fixed set.
use netray_engine::catalogue::catalogue;
use netray_engine::profile::Profiles;
use netray_model::{CheckId, Protocol, Status};
use netray_web::spectrum::{
    Label, LineTone, RunInput, RunLayout, SectionState, label_width, layout, line_pitch_min,
    scaled_start,
};
use proptest::prelude::*;
use std::collections::BTreeMap;

const TOL: f64 = 1e-6;
const LINES: usize = 57;
const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

fn status() -> impl Strategy<Value = Option<Status>> {
    prop_oneof![
        2 => Just(None),
        3 => Just(Some(Status::Pass)),
        2 => Just(Some(Status::Warn)),
        2 => Just(Some(Status::Fail)),
        1 => Just(Some(Status::NotApplicable)),
        1 => Just(Some(Status::NotTested)),
        1 => Just(Some(Status::Unmeasured)),
    ]
}

fn section() -> impl Strategy<Value = SectionState> {
    prop_oneof![
        8 => Just(SectionState::Measured),
        1 => Just(SectionState::NotApplicable),
        1 => Just(SectionState::Unmeasured),
    ]
}

/// Statuses by catalogue position, plus one state per protocol.
fn run() -> impl Strategy<Value = (Vec<Option<Status>>, Vec<SectionState>)> {
    (
        prop::collection::vec(status(), LINES),
        prop::collection::vec(section(), PROTOCOLS.len()),
    )
}

type RawRun = (Vec<Option<Status>>, Vec<SectionState>);

fn input<'a>(raw: &RawRun, profile: &'a netray_engine::profile::Profile) -> RunInput<'a> {
    let statuses: BTreeMap<CheckId, Status> = catalogue()
        .entries()
        .iter()
        .zip(&raw.0)
        .filter_map(|(e, s)| s.map(|s| (e.id.clone(), s)))
        .collect();
    let sections: BTreeMap<Protocol, SectionState> = PROTOCOLS
        .iter()
        .copied()
        .zip(raw.1.iter().copied())
        .collect();
    RunInput {
        statuses,
        sections,
        profile,
    }
}

/// The spec's definition of measured: Pass/Warn/Fail, weighted in the profile, section Measured
/// (a section with a weighted unmeasured check is unscored, so Unmeasured; requirement 12).
fn measured(run: &RunInput, id: &CheckId, protocol: Protocol) -> Option<Status> {
    let s = *run.statuses.get(id)?;
    let weighted = run
        .profile
        .sections
        .get(&protocol)
        .is_some_and(|sec| sec.checks.contains_key(id));
    let unscored = run.profile.sections.get(&protocol).is_some_and(|sec| {
        sec.checks
            .keys()
            .any(|c| run.statuses.get(c) == Some(&Status::Unmeasured))
    });
    let state = match run
        .sections
        .get(&protocol)
        .copied()
        .unwrap_or(SectionState::Measured)
    {
        SectionState::Measured if unscored => SectionState::Unmeasured,
        other => other,
    };
    (matches!(s, Status::Pass | Status::Warn | Status::Fail)
        && weighted
        && state == SectionState::Measured)
        .then_some(s)
}

fn overlap(a_start: f64, a_w: f64, b_start: f64, b_w: f64) -> bool {
    a_start < b_start + b_w - TOL && b_start < a_start + a_w - TOL
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn check_labels(
    r: &RunLayout,
    shorts: &[String],
    plate: f64,
    pitch: f64,
    plate2: f64,
) -> Result<(), TestCaseError> {
    let n = shorts.len();
    let ratio = plate2 / plate;

    // (b) labelled set equals warn/fail/fixed, one each, text = short
    let expected: Vec<usize> = r
        .tones
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, LineTone::Warn | LineTone::Fail | LineTone::Fixed))
        .map(|(i, _)| i)
        .collect();
    let got: Vec<usize> = r.labels.iter().map(|l| l.line).collect();
    prop_assert_eq!(&got, &expected);
    for l in &r.labels {
        prop_assert_eq!(&l.text, &shorts[l.line]);
        prop_assert!((l.width - label_width(l.text.chars().count())).abs() < TOL);
    }

    // (a) no overlap within a row, also scaled by the ratio
    for (i, a) in r.labels.iter().enumerate() {
        for b in &r.labels[i + 1..] {
            if a.row == b.row {
                prop_assert!(
                    !overlap(a.start, a.width, b.start, b.width),
                    "{a:?} vs {b:?}"
                );
                prop_assert!(
                    !overlap(a.start * ratio, a.width, b.start * ratio, b.width),
                    "{a:?} vs {b:?} at scale {ratio}"
                );
            }
        }
    }

    // (d) rows >= 3, rows bound labels, no gratuitous row
    prop_assert!(r.rows >= 3);
    for l in &r.labels {
        prop_assert!(l.row < r.rows);
        for j in 0..l.row {
            let hit = r
                .labels
                .iter()
                .any(|o: &Label| o.row == j && overlap(l.start, l.width, o.start, o.width));
            prop_assert!(
                hit,
                "label {l:?} in row {} overlaps nothing in row {j}",
                l.row
            );
        }
    }

    // (e) boxes within the plate; (f) and within the wider plate after scaling
    for l in &r.labels {
        if l.width <= plate {
            prop_assert!(l.start >= -TOL && l.start + l.width <= plate + TOL, "{l:?}");
        }
        if l.width <= plate2 {
            let s = l.start * ratio;
            prop_assert!(s >= -TOL && s + l.width <= plate2 + TOL, "{l:?} scaled");
        }
        prop_assert!((l.leader - (l.line as f64 + 0.5) * pitch).abs() < TOL);
    }
    prop_assert!(ratio >= 1.0 - TOL);
    prop_assert!(n == LINES);

    // (g) clamped scaling keeps every label on its leader and rows free of overlap
    for l in &r.labels {
        prop_assert!(
            (scaled_start(l, plate, plate) - l.start).abs() < TOL,
            "{l:?} unclamped"
        );
    }
    for (i, a) in r.labels.iter().enumerate() {
        let sa = scaled_start(a, plate, plate2);
        prop_assert!(sa >= -TOL);
        if a.width <= plate2 {
            prop_assert!(sa + a.width <= plate2 + TOL);
            let lx = a.leader * ratio;
            prop_assert!(
                sa <= lx + TOL && lx <= sa + a.width + TOL,
                "{a:?} left its leader"
            );
        }
        for b in &r.labels[i + 1..] {
            if a.row == b.row {
                let sb = scaled_start(b, plate, plate2);
                prop_assert!(!overlap(sa, a.width, sb, b.width), "{a:?} vs {b:?} clamped");
            }
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn catalogue_layout_claims_hold(
        cur_raw in run(),
        old_raw in run(),
        w in 100u32..=2400,
        extra in 0u32..=300,
    ) {
        let profile = Profiles::current();
        let cur = input(&cur_raw, profile);
        let old = input(&old_raw, profile);
        let l = layout(&cur, std::slice::from_ref(&old), catalogue(), w).expect("lines");
        let l2 = layout(&cur, std::slice::from_ref(&old), catalogue(), w + extra).expect("lines");

        // exactly 57 lines, one entry per run, plate and pitch of requirement 2
        prop_assert_eq!(l.lines.len(), LINES);
        prop_assert_eq!(l.runs.len(), 2);
        let min = 8.0;
        prop_assert!(line_pitch_min() >= min - TOL && line_pitch_min() <= min + TOL);
        if f64::from(w) >= LINES as f64 * min {
            prop_assert!((l.plate_width - f64::from(w)).abs() < TOL);
            prop_assert!((l.pitch - f64::from(w) / LINES as f64).abs() < TOL);
        } else {
            prop_assert!((l.plate_width - LINES as f64 * min).abs() < TOL);
            prop_assert!((l.pitch - min).abs() < TOL);
        }

        // identical x for both runs: lines carry x once, every leader sits on it
        for (i, line) in l.lines.iter().enumerate() {
            prop_assert!((line.x - (i as f64 + 0.5) * l.pitch).abs() < TOL);
        }
        let shorts: Vec<String> = l.lines.iter().map(|x| x.short.clone()).collect();
        for r in &l.runs {
            prop_assert_eq!(r.tones.len(), LINES);
            prop_assert_eq!(r.heights.len(), LINES);
            for lb in &r.labels {
                prop_assert!((lb.leader - l.lines[lb.line].x).abs() < TOL);
            }
            check_labels(r, &shorts, l.plate_width, l.pitch, l2.plate_width)?;
        }

        // heights in (0, 1]; muted lines no taller than any measured line of their run
        for (r, run) in l.runs.iter().zip([&cur, &old]) {
            for h in &r.heights {
                prop_assert!(*h > 0.0 && *h <= 1.0 + TOL, "height {h}");
            }
            let max_muted = r
                .tones
                .iter()
                .zip(&r.heights)
                .filter(|(t, _)| **t == LineTone::Muted)
                .map(|(_, h)| *h)
                .fold(0.0, f64::max);
            for (i, (t, h)) in r.tones.iter().zip(&r.heights).enumerate() {
                if *t != LineTone::Muted {
                    prop_assert!(max_muted <= *h + TOL, "line {i}: muted {max_muted} > measured {h}");
                }
                let line = &l.lines[i];
                let is_measured = measured(run, &line.id, line.protocol).is_some();
                prop_assert_eq!(*t != LineTone::Muted, is_measured, "line {}", i);
            }
        }

        // a label on every Fixed line; the Fixed set is the Warn/Fail -> Pass pairs
        let fixed: Vec<usize> = l.runs[0]
            .tones
            .iter()
            .enumerate()
            .filter(|(_, t)| **t == LineTone::Fixed)
            .map(|(i, _)| i)
            .collect();
        for i in &fixed {
            prop_assert!(l.runs[0].labels.iter().any(|lb| lb.line == *i), "fixed {i} unlabelled");
        }
        let expected: Vec<usize> = l
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                matches!(
                    measured(&old, &line.id, line.protocol),
                    Some(Status::Warn | Status::Fail)
                ) && measured(&cur, &line.id, line.protocol) == Some(Status::Pass)
            })
            .map(|(i, _)| i)
            .collect();
        prop_assert_eq!(fixed, expected);
        prop_assert!(!l.runs[1].tones.contains(&LineTone::Fixed));
    }
}
