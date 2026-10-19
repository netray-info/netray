//! Table scenarios of the spectrum layout core (V2 spec v2-spectrum, Phase 1, requirements 2-7).
use netray_web::spectrum::{
    LayoutError, LineInput, LineTone, Placement, label_width, line_pitch_min, place, scaled_start,
};
use netray_web::tokens::values;

const EPS: f64 = 1e-6;

fn near(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() < eps
}

fn line(short: &str, group: usize, tone: LineTone) -> LineInput {
    LineInput {
        short: short.to_string(),
        group,
        tone,
    }
}

/// `n` pass lines in one group, with `(index, short, tone)` overrides.
fn lines(n: usize, over: &[(usize, &str, LineTone)]) -> Vec<LineInput> {
    let mut v: Vec<LineInput> = (0..n).map(|_| line("ok", 0, LineTone::Pass)).collect();
    for (i, s, t) in over {
        v[*i] = line(s, 0, *t);
    }
    v
}

fn place_ok(lines: &[LineInput], width: u32) -> Placement {
    place(lines, width).expect("placement")
}

/// Scale a placement to the plate of `wide` and assert requirement 7.
fn assert_scaled_valid(p: &Placement, wide: &Placement) {
    let r = wide.plate_width / p.plate_width;
    for (k, a) in p.labels.iter().enumerate() {
        let sa = a.start * r;
        if a.width <= wide.plate_width {
            assert!(
                sa >= -EPS && sa + a.width <= wide.plate_width + EPS,
                "label {k} outside plate"
            );
        }
        for b in &p.labels[k + 1..] {
            if a.row != b.row {
                continue;
            }
            let sb = b.start * r;
            let overlap = sa < sb + b.width - EPS && sb < sa + a.width - EPS;
            assert!(
                !overlap,
                "labels on lines {} and {} overlap in row {}",
                a.line, b.line, a.row
            );
        }
    }
}

#[test]
fn c2_plate_and_pitch_rule() {
    let min: f64 = values::SPECTRUM_LINE_PITCH_MIN
        .strip_suffix("px")
        .expect("px token")
        .parse()
        .expect("number");
    assert_eq!(line_pitch_min(), 8.0);
    assert_eq!(line_pitch_min(), min);
    // (n, width, plate, pitch)
    let table: [(usize, u32, f64, f64); 5] = [
        (10, 100, 100.0, 10.0),
        (10, 80, 80.0, 8.0),
        (10, 79, 80.0, 8.0),
        (1, 5, 8.0, 8.0),
        (4, 1000, 1000.0, 250.0),
    ];
    for (n, w, plate, pitch) in table {
        let p = place_ok(&lines(n, &[]), w);
        assert!(near(p.plate_width, plate, 1e-9), "plate n={n} w={w}");
        assert!(near(p.pitch, pitch, 1e-9), "pitch n={n} w={w}");
        assert!(p.pitch >= min);
    }
}

#[test]
fn c3_x_groups_and_boundaries() {
    // groups of 2, 3 and 1 lines at pitch 100
    let ls: Vec<LineInput> = [0, 0, 1, 1, 1, 2]
        .iter()
        .map(|g| line("ok", *g, LineTone::Pass))
        .collect();
    let p = place_ok(&ls, 600);
    let xs = [50.0, 150.0, 250.0, 350.0, 450.0, 550.0];
    assert_eq!(p.xs.len(), 6);
    for (x, e) in p.xs.iter().zip(xs) {
        assert!(near(*x, e, 1e-9));
    }
    let groups: Vec<(usize, usize, f64, f64)> = p
        .groups
        .iter()
        .map(|g| (g.first, g.len, g.start, g.width))
        .collect();
    assert_eq!(
        groups,
        vec![
            (0, 2, 0.0, 200.0),
            (2, 3, 200.0, 300.0),
            (5, 1, 500.0, 100.0)
        ]
    );
    assert_eq!(p.boundaries, vec![200.0, 500.0]);

    // below the minimum pitch: plate 48, pitch 8
    let p = place_ok(&ls, 30);
    assert_eq!(p.boundaries, vec![16.0, 40.0]);
    assert_eq!(p.groups[1].width, 24.0);
}

#[test]
fn c4_label_box_edge_shift_and_leader() {
    assert!(near(label_width(4), 33.76, 1e-9));
    // 4 lines at 400 px: pitch 100, plate 400
    let ls = lines(
        4,
        &[(0, "abcd", LineTone::Fail), (3, "abcdefgh", LineTone::Fail)],
    );
    let p = place_ok(&ls, 400);
    let l0 = &p.labels[0];
    assert!(near(l0.start, 50.0, EPS) && near(l0.width, 33.76, 1e-9) && near(l0.leader, 50.0, EPS));
    // line 3 at x 350, 8 chars = 63.52 px: 350 + 63.52 > 400 -> start 336.48, leader stays at 350
    let l3 = &p.labels[1];
    assert!(near(l3.start, 336.48, EPS) && near(l3.leader, 350.0, EPS));
    // over-wide: 8 chars = 63.52 px in a 32 px plate starts at 0
    let p = place_ok(&lines(4, &[(2, "abcdefgh", LineTone::Fail)]), 32);
    assert_eq!(p.labels[0].start, 0.0);
    assert!(near(p.labels[0].leader, 20.0, EPS));
}

#[test]
fn c5_greedy_rows_and_nothing_dropped() {
    // 54 lines at 432 px, pitch 8; fails on lines 0-5 with 4 chars (33.76 px boxes at 4, 12, ..., 44)
    let over: Vec<(usize, &str, LineTone)> = (0..6).map(|i| (i, "abcd", LineTone::Fail)).collect();
    let p = place_ok(&lines(54, &over), 432);
    let rows: Vec<usize> = p.labels.iter().map(|l| l.row).collect();
    // line 4 (start 36) overlaps rows 0-3 and opens row 4; line 5 (start 44) clears box 0 (ends 37.76)
    assert_eq!(rows, vec![0, 1, 2, 3, 4, 0]);
    assert_eq!(p.rows, 5);
    assert_eq!(p.labels.len(), 6);
}

#[test]
fn c6_labelled_set_and_text() {
    let ls = vec![
        line("pa", 0, LineTone::Pass),
        line("wa", 0, LineTone::Warn),
        line("fa", 0, LineTone::Fail),
        line("fx", 0, LineTone::Fixed),
        line("mu", 0, LineTone::Muted),
    ];
    let p = place_ok(&ls, 1000);
    let got: Vec<(usize, &str)> = p.labels.iter().map(|l| (l.line, l.text.as_str())).collect();
    assert_eq!(got, vec![(1, "wa"), (2, "fa"), (3, "fx")]);
}

#[test]
fn c10_wide_plate_pitch_and_first_x() {
    let p = place_ok(&lines(54, &[]), 1056);
    assert_eq!(p.plate_width, 1056.0);
    assert!(near(p.pitch, 1056.0 / 54.0, 1e-9));
    assert!(near(p.pitch, 19.56, 0.01));
    assert!(near(p.xs[0], 9.78, 0.01));
    assert!(near(p.xs[0], 1056.0 / 108.0, 1e-9));
}

#[test]
fn c11_minimum_pitch_floor() {
    for w in [432, 431, 358] {
        let p = place_ok(&lines(54, &[]), w);
        assert_eq!(p.plate_width, 432.0, "width {w}");
        assert_eq!(p.pitch, 8.0, "width {w}");
    }
}

#[test]
fn c12_no_lines_is_refused() {
    assert_eq!(place(&[], 432), Err(LayoutError::NoLines));
}

#[test]
fn c13_four_labels_four_rows() {
    let over: Vec<(usize, &str, LineTone)> = (0..4).map(|i| (i, "abcd", LineTone::Fail)).collect();
    let p = place_ok(&lines(54, &over), 432);
    let rows: Vec<usize> = p.labels.iter().map(|l| l.row).collect();
    assert_eq!(rows, vec![0, 1, 2, 3]);
    assert_eq!(p.rows, 4);
}

#[test]
fn c14_fourth_label_returns_to_row_zero() {
    // pitch 12: starts 6, 18, 30, 42; box 0 ends at 39.76 < 42
    let over: Vec<(usize, &str, LineTone)> = (0..4).map(|i| (i, "abcd", LineTone::Fail)).collect();
    let p = place_ok(&lines(54, &over), 648);
    assert!(near(p.pitch, 12.0, 1e-9));
    let starts: Vec<f64> = p.labels.iter().map(|l| l.start).collect();
    for (s, e) in starts.iter().zip([6.0, 18.0, 30.0, 42.0]) {
        assert!(near(*s, e, EPS));
    }
    let rows: Vec<usize> = p.labels.iter().map(|l| l.row).collect();
    assert_eq!(rows, vec![0, 1, 2, 0]);
    assert_eq!(p.rows, 3);
}

#[test]
fn c15_single_warn_row_zero_three_rows() {
    let p = place_ok(&lines(54, &[(10, "warn", LineTone::Warn)]), 432);
    assert_eq!(p.labels.len(), 1);
    assert_eq!(p.labels[0].row, 0);
    assert_eq!(p.rows, 3);
}

#[test]
fn c16_right_edge_shift() {
    // last line x = 53.5 * 8 = 428; 8 chars = 63.52 px; 432 - 63.52 = 368.48
    let p = place_ok(&lines(54, &[(53, "abcdefgh", LineTone::Fail)]), 432);
    let l = &p.labels[0];
    assert!(near(l.width, 63.52, 1e-9));
    assert!(near(l.start, 368.48, EPS));
    assert!(near(l.leader, 428.0, EPS));
    // a box that fits starts at its line: line 10, x = 84
    let p = place_ok(&lines(54, &[(10, "abcd", LineTone::Fail)]), 432);
    assert!(near(p.labels[0].start, 84.0, EPS));
}

#[test]
fn c17_over_wide_box_starts_at_zero() {
    let p = place_ok(&lines(4, &[(3, "abcdefgh", LineTone::Fail)]), 32);
    assert_eq!(p.plate_width, 32.0);
    assert_eq!(p.labels[0].start, 0.0);
}

#[test]
fn c18_only_fixed_is_labelled() {
    let ls = vec![
        line("p", 0, LineTone::Pass),
        line("m", 0, LineTone::Muted),
        line("f", 0, LineTone::Fixed),
    ];
    let p = place_ok(&ls, 300);
    assert_eq!(p.labels.len(), 1);
    assert_eq!(p.labels[0].line, 2);
    assert_eq!(p.labels[0].text, "f");
}

#[test]
fn c19_touching_boxes_share_a_row() {
    // 54 lines at 432 px, pitch 8. Line 0 (x 4) carries a 50-char label:
    // width 50 * 0.62 * 12 + 4 = 376, box [4, 380). Line 47 sits at x 47.5 * 8 = 380,
    // so the box of line 47 starts exactly where line 0's box ends.
    let long = "x".repeat(50);
    let p = place_ok(
        &lines(
            54,
            &[(0, &long, LineTone::Fail), (47, "abcd", LineTone::Fail)],
        ),
        432,
    );
    let (a, b) = (&p.labels[0], &p.labels[1]);
    assert!(
        near(a.start + a.width, b.start, 1e-6),
        "construction must touch"
    );
    assert_eq!(a.row, 0);
    assert_eq!(b.row, 0);
    assert_eq!(p.rows, 3);
}

#[test]
fn c20_dense_placement_scales_to_wider_plate() {
    let tones = [
        LineTone::Fail,
        LineTone::Fail,
        LineTone::Warn,
        LineTone::Fixed,
        LineTone::Fail,
    ];
    let shorts = ["a", "abcd", "abcdefgh", "ab c", "abcdef", "xyz", "abcdefg"];
    let ls: Vec<LineInput> = (0..54)
        .map(|i| line(shorts[i % shorts.len()], 0, tones[i % tones.len()]))
        .collect();
    let narrow = place_ok(&ls, 432);
    assert!(narrow.rows > 3, "dense case must stack rows");
    assert_eq!(narrow.labels.len(), 54);
    let wide = place_ok(&ls, 1056);
    assert_eq!(wide.plate_width, 1056.0);
    assert_scaled_valid(&narrow, &wide);
}

/// Requirement 7's `scaled_start`: an over-wide box stays at 0 at a wider plate it still exceeds,
/// and an edge-shifted box returns to its leader once the plate leaves room.
#[test]
fn c7_scaled_start_clamps_over_wide_and_shifted_boxes() {
    // c17's input: 4 lines at 32 px, 8 chars (63.52 px) on line 3, leader 28
    let p = place_ok(&lines(4, &[(3, "abcdefgh", LineTone::Fail)]), 32);
    let l = &p.labels[0];
    assert_eq!(scaled_start(l, 32.0, 32.0), 0.0);
    assert_eq!(scaled_start(l, 32.0, 40.0), 0.0);
    // 54 lines at 432 px, last line (x 428) with 6 chars (48.64 px): shifted to 383.36
    let p = place_ok(&lines(54, &[(53, "FCrDNS", LineTone::Warn)]), 432);
    let l = &p.labels[0];
    assert!(near(scaled_start(l, 432.0, 432.0), 383.36, EPS));
    // at 1056 px the leader is at 1046.22; the box ends at the edge and spans its leader
    let s = scaled_start(l, 432.0, 1056.0);
    assert!(near(s, 1056.0 - 48.64, EPS));
    assert!(s <= 428.0 * 1056.0 / 432.0 && 428.0 * 1056.0 / 432.0 <= s + l.width);
}
