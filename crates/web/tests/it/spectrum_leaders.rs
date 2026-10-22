//! Leader clearance of the spectrum layout (V2 spec v2-ui-polish, requirement 9, criteria C2, C8,
//! C9): a leader rises from the plate through every lower row, so no box in a lower row covers it.
use netray_web::spectrum::{LineInput, LineTone, Placement, place};
use proptest::prelude::*;

const EPS: f64 = 0.5;

fn covers(box_start: f64, box_width: f64, x: f64) -> bool {
    x > box_start + EPS && x < box_start + box_width - EPS
}

/// No box in row r covers the leader of a label in a row above r, except where two boxes each
/// cover the other's leader (two boxes pushed in from the plate's right edge): no row order
/// clears both.
fn assert_leaders_clear(p: &Placement) {
    for a in &p.labels {
        for b in &p.labels {
            let mutual = covers(a.start, a.width, b.leader) && covers(b.start, b.width, a.leader);
            if a.row < b.row && !mutual {
                assert!(
                    !covers(a.start, a.width, b.leader),
                    "box {a:?} covers the leader of {b:?}"
                );
            }
        }
    }
}

fn assert_row_disjoint(p: &Placement) {
    for (i, a) in p.labels.iter().enumerate() {
        for b in &p.labels[i + 1..] {
            if a.row == b.row {
                assert!(
                    !(a.start < b.start + b.width - EPS && b.start < a.start + a.width - EPS),
                    "overlap in row {}: {a:?} vs {b:?}",
                    a.row
                );
            }
        }
    }
}

fn line(short: &str, tone: LineTone) -> LineInput {
    LineInput {
        short: short.to_string(),
        group: 0,
        tone,
    }
}

#[test]
fn a_later_leader_inside_an_earlier_box_leaves_row_zero_clear() {
    // Passing lines on both sides keep both boxes inside the plate, away from its edges.
    let mut lines: Vec<LineInput> = (0..5).map(|_| line("pass", LineTone::Pass)).collect();
    lines.push(line("expiry", LineTone::Warn));
    lines.push(line("ECH", LineTone::Fail));
    lines.extend((0..5).map(|_| line("pass", LineTone::Pass)));
    let p = place(&lines, 1).unwrap();
    let first = p.labels.iter().find(|l| l.line == 5).unwrap();
    let second = p.labels.iter().find(|l| l.line == 6).unwrap();
    assert!(
        covers(first.start, first.width, second.leader),
        "scenario precondition: the first box must reach the second leader"
    );
    assert_ne!(first.row, second.row);
    assert_leaders_clear(&p);
}

fn tone() -> impl Strategy<Value = LineTone> {
    prop_oneof![
        Just(LineTone::Pass),
        Just(LineTone::Warn),
        Just(LineTone::Fail),
        Just(LineTone::Fixed),
        Just(LineTone::Muted),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn leaders_stay_clear_and_every_flagged_line_keeps_a_label(
        rows in prop::collection::vec(("[A-Za-z0-9]{3,8}", tone()), 2..=40),
        width in 1u32..=1600,
    ) {
        let lines: Vec<LineInput> = rows.iter().map(|(s, t)| line(s, *t)).collect();
        let p = place(&lines, width).unwrap();
        let flagged = lines
            .iter()
            .filter(|l| matches!(l.tone, LineTone::Warn | LineTone::Fail | LineTone::Fixed))
            .count();
        prop_assert_eq!(p.labels.len(), flagged);
        prop_assert!(p.rows >= 3);
        assert_row_disjoint(&p);
        assert_leaders_clear(&p);
    }
}
