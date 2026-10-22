//! Property test of the spectrum layout (V2 spec v2-spectrum, requirement 8, criteria C7, C8,
//! C21): requirements 2-7 over random line lists and widths.
use netray_web::spectrum::{
    LineInput, LineTone, Placement, at_leader, covers, label_width, line_pitch_min, place,
    scaled_start,
};
use proptest::prelude::*;
use std::path::PathBuf;

const TOL: f64 = 1e-6;

fn tone() -> impl Strategy<Value = LineTone> {
    prop_oneof![
        Just(LineTone::Pass),
        Just(LineTone::Warn),
        Just(LineTone::Fail),
        Just(LineTone::Fixed),
        Just(LineTone::Muted),
    ]
}

/// 1-120 lines, shorts of 1-8 characters, groups as nondecreasing contiguous runs.
fn lines() -> impl Strategy<Value = Vec<LineInput>> {
    prop::collection::vec(("[A-Za-z0-9 ]{1,8}", tone(), any::<bool>()), 1..=120).prop_map(|rows| {
        let mut group = 0;
        rows.into_iter()
            .enumerate()
            .map(|(i, (short, tone, new_group))| {
                if i > 0 && new_group {
                    group += 1;
                }
                LineInput { short, group, tone }
            })
            .collect()
    })
}

fn overlap(a_start: f64, a_w: f64, b_start: f64, b_w: f64) -> bool {
    a_start < b_start + b_w - TOL && b_start < a_start + a_w - TOL
}

/// No two boxes in a row overlap, after scaling every start by `r`; widths stay.
fn assert_no_row_overlap(p: &Placement, r: f64) {
    for (i, a) in p.labels.iter().enumerate() {
        for b in &p.labels[i + 1..] {
            if a.row == b.row {
                assert!(
                    !overlap(a.start * r, a.width, b.start * r, b.width),
                    "overlap in row {}: {a:?} vs {b:?} at scale {r}",
                    a.row
                );
            }
        }
    }
}

/// Every box no wider than `plate` lies within [0, plate], after scaling starts by `r`.
fn assert_inside(p: &Placement, r: f64, plate: f64) {
    for l in &p.labels {
        if l.width <= plate {
            let s = l.start * r;
            assert!(s >= -TOL, "starts left of the plate: {l:?} at scale {r}");
            assert!(
                s + l.width <= plate + TOL,
                "ends right of the plate {plate}: {l:?} at scale {r}"
            );
        }
    }
}

/// 256 cases with failure persistence off, so a failing run writes no regression file (C21).
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn layout_claims_hold(input in lines(), w in 100u32..=2400, extra in 0u32..=300) {
        let n = input.len();
        let p = place(&input, w).expect("n >= 1 lines are placed");

        // (c) plate and pitch of requirement 2; the minimum pitch is the token's 8px
        prop_assert!(line_pitch_min() >= 8.0 - TOL && line_pitch_min() <= 8.0 + TOL);
        let min = 8.0;
        if f64::from(w) >= n as f64 * min {
            prop_assert!((p.plate_width - f64::from(w)).abs() < TOL);
            prop_assert!((p.pitch - f64::from(w) / n as f64).abs() < TOL);
        } else {
            prop_assert!((p.plate_width - n as f64 * min).abs() < TOL);
            prop_assert!((p.pitch - min).abs() < TOL);
        }
        prop_assert!(p.pitch >= line_pitch_min() - TOL);
        prop_assert_eq!(p.xs.len(), n);

        // (b) labelled set equals warn/fail/fixed, one each, text = short
        let expected: Vec<usize> = input
            .iter()
            .enumerate()
            .filter(|(_, l)| matches!(l.tone, LineTone::Warn | LineTone::Fail | LineTone::Fixed))
            .map(|(i, _)| i)
            .collect();
        let got: Vec<usize> = p.labels.iter().map(|l| l.line).collect();
        prop_assert_eq!(&got, &expected);
        for l in &p.labels {
            prop_assert_eq!(&l.text, &input[l.line].short);
            prop_assert!((l.width - label_width(l.text.chars().count())).abs() < TOL);
        }

        // (a) no overlap within a row
        assert_no_row_overlap(&p, 1.0);

        // (d) rows >= 3, rows bound labels, no gratuitous row
        prop_assert!(p.rows >= 3);
        for l in &p.labels {
            prop_assert!(l.row < p.rows);
            for j in 0..l.row {
                // a row is skipped for an overlap there, or to clear a leader the box covers
                let hit = p.labels.iter().any(|o| {
                    (o.row == j && overlap(l.start, l.width, o.start, o.width))
                        || (at_leader(l) && o.row >= j && covers(l, o))
                });
                prop_assert!(hit, "label {l:?} in row {} overlaps nothing in row {j}", l.row);
            }
        }

        // (e) boxes within the plate, leaders at the line's x
        assert_inside(&p, 1.0, p.plate_width);
        for (i, x) in p.xs.iter().enumerate() {
            prop_assert!((x - (i as f64 + 0.5) * p.pitch).abs() < TOL);
        }
        for l in &p.labels {
            prop_assert!((l.leader - p.xs[l.line]).abs() < TOL);
            prop_assert!((l.leader - (l.line as f64 + 0.5) * p.pitch).abs() < TOL);
        }

        // (f) requirement 7: the placement scaled to a wider W' stays valid
        let w2 = w + extra;
        let p2 = place(&input, w2).expect("n >= 1 lines are placed");
        let r = p2.plate_width / p.plate_width;
        prop_assert!(r >= 1.0 - TOL);
        assert_no_row_overlap(&p, r);
        assert_inside(&p, r, p2.plate_width);

        // (g) requirement 7: clamped at the wider plate, every label stays on its leader
        let p_w = p.plate_width;
        let q_w = p2.plate_width;
        for l in &p.labels {
            prop_assert!((scaled_start(l, p_w, p_w) - l.start).abs() < TOL, "{l:?} unclamped");
        }
        for (i, a) in p.labels.iter().enumerate() {
            let sa = scaled_start(a, p_w, q_w);
            prop_assert!(sa >= -TOL);
            if a.width <= q_w {
                prop_assert!(sa + a.width <= q_w + TOL, "{a:?} ends right of {q_w}");
                let lx = a.leader * r;
                prop_assert!(sa <= lx + TOL && lx <= sa + a.width + TOL, "{a:?} left its leader");
            }
            for b in &p.labels[i + 1..] {
                if a.row == b.row {
                    let sb = scaled_start(b, p_w, q_w);
                    prop_assert!(!overlap(sa, a.width, sb, b.width), "{a:?} vs {b:?} clamped");
                }
            }
        }
    }
}

/// C21: the gate writes no file into the checkout. A passing run writes none under any config,
/// so the config itself is asserted, and no regression file of either proptest layout exists.
#[test]
fn no_proptest_regressions_dir() {
    let c = config();
    assert!(c.failure_persistence.is_none(), "failure persistence is on");
    assert_eq!(c.cases, 256);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for sub in ["", "tests", "src"] {
        assert!(
            !dir.join(sub).join("proptest-regressions").exists(),
            "proptest-regressions found under {}",
            dir.join(sub).display()
        );
    }
    let regressions: Vec<_> = std::fs::read_dir(dir.join("tests"))
        .expect("crates/web/tests is readable")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .ends_with(".proptest-regressions")
        })
        .collect();
    assert!(regressions.is_empty(), "regression files: {regressions:?}");
}
