//! Leader clearance of the spectrum layout (V2 spec v2-ui-polish, requirement 9, criteria C2, C8,
//! C9): a leader rises from the plate through every lower row, so no box in a lower row covers it.
use netray_web::spectrum::{LineInput, LineTone, Placement, place};
use proptest::prelude::*;

const EPS: f64 = 0.5;

fn covers(box_start: f64, box_width: f64, x: f64) -> bool {
    x > box_start + EPS && x < box_start + box_width - EPS
}

/// No box in row r covers the leader of a label in a row above r, except a box pushed in from the
/// plate's right edge (or wider than the plate), which does not stand at its own leader: no row
/// order clears the leaders it spans to its left.
fn assert_leaders_clear(p: &Placement) {
    for a in &p.labels {
        for b in &p.labels {
            let shifted = (a.start - a.leader).abs() > 1e-6;
            if a.row < b.row && !shifted {
                assert!(
                    !covers(a.start, a.width, b.leader),
                    "box {a:?} covers the leader of {b:?}"
                );
            }
        }
    }
}

/// The line-order rows: each label, in line order, in the lowest row where its box overlaps no
/// earlier label's box (the rule a dense cluster falls back to, SDD §5.5 point 2).
fn line_order_rows(p: &Placement) -> Vec<usize> {
    let mut rows: Vec<usize> = Vec::new();
    for (i, l) in p.labels.iter().enumerate() {
        let mut row = 0;
        while p.labels[..i].iter().zip(&rows).any(|(o, &r)| {
            r == row && l.start < o.start + o.width - 1e-6 && o.start < l.start + l.width - 1e-6
        }) {
            row += 1;
        }
        rows.push(row);
    }
    rows
}

/// The cluster index of each label: labels whose boxes intersect, directly or through others.
fn cluster_of(p: &Placement) -> Vec<usize> {
    let mut order: Vec<usize> = (0..p.labels.len()).collect();
    order.sort_by(|&i, &j| p.labels[i].start.total_cmp(&p.labels[j].start));
    let mut out = vec![0; p.labels.len()];
    let (mut cluster, mut end) = (0, f64::NEG_INFINITY);
    for (k, &i) in order.iter().enumerate() {
        let l = &p.labels[i];
        if k == 0 || l.start >= end - 1e-6 {
            cluster += usize::from(k > 0);
            end = l.start + l.width;
        } else {
            end = end.max(l.start + l.width);
        }
        out[i] = cluster;
    }
    out
}

/// A box at its leader crosses a higher leader only inside a cluster placed in line order.
fn crossings_only_in_line_order_clusters(p: &Placement) -> bool {
    let rows = line_order_rows(p);
    let clusters = cluster_of(p);
    p.labels.iter().enumerate().all(|(i, a)| {
        let shifted = (a.start - a.leader).abs() > 1e-6;
        let crosses = !shifted
            && p.labels
                .iter()
                .any(|b| a.row < b.row && covers(a.start, a.width, b.leader));
        !crosses
            || p.labels
                .iter()
                .enumerate()
                .filter(|(j, _)| clusters[*j] == clusters[i])
                .all(|(j, l)| l.row == rows[j])
    })
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
        // leaders clear, or a dense cluster that falls back to the line-order placement
        prop_assert!(crossings_only_in_line_order_clusters(&p), "{p:?}");
    }
}
