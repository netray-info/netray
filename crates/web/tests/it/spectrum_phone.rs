//! Spectrum labels placed at the phone plate (spec v2-spectrum-phone, requirements 1 to 3): the
//! layout places every run's labels a second time at the 390 px phone's plate, the component
//! draws that placement at phone width, and the dense fixture runs cross no leader there.
use std::fs;
use std::path::PathBuf;

use netray_engine::catalogue;
use netray_model::run::Run;
use netray_model::{CheckId, Status};
use netray_web::components::{SpectrumView, spectrum};
use netray_web::pages::result::views::layout_input_or_current;
use netray_web::spectrum::{
    Label, LineInput, LineTone, RunInput, layout, phone_label_width, phone_plate, place_phone,
};

/// The plate width the page places at: 57 checks at the narrowest pitch.
const W: u32 = 456;

fn fixture(name: &str) -> Run {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history")
        .join(name);
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    serde_json::from_value(v["run"].clone()).unwrap()
}

/// Every history fixture as the result page lays it out: the current entry over its older ones.
fn pages() -> Vec<(&'static str, Vec<Run>)> {
    vec![
        ("example.com", vec![fixture("example.com.json")]),
        ("stale.example.com", vec![fixture("stale.example.com.json")]),
        ("new.example.com", vec![fixture("new.example.com.json")]),
        (
            "checkout.eu-central.storefront.example.com",
            vec![fixture("checkout.eu-central.storefront.example.com.json")],
        ),
        (
            "shop.example.com",
            vec![
                fixture("shop.example.com-3.json"),
                fixture("shop.example.com-2.json"),
                fixture("shop.example.com-1.json"),
            ],
        ),
    ]
}

fn covers(a: &Label, b: &Label) -> bool {
    b.leader > a.start + 0.5 && b.leader < a.start + a.width - 0.5
}

/// The pairs where a box at its leader covers the leader of a label in a higher row, or two boxes
/// of one row overlap.
fn crossings(labels: &[Label]) -> Vec<String> {
    let mut out = Vec::new();
    for a in labels {
        for b in labels {
            let at_leader = (a.start - a.leader).abs() <= 1e-6;
            if a.row < b.row && at_leader && covers(a, b) {
                out.push(format!("{} (row {}) covers {} (row {})", a.text, a.row, b.text, b.row));
            }
            if a.line < b.line
                && a.row == b.row
                && a.start < b.start + b.width - 1e-6
                && b.start < a.start + a.width - 1e-6
            {
                out.push(format!("{} overlaps {} in row {}", a.text, b.text, a.row));
            }
        }
    }
    out
}

#[test]
fn c1_the_phone_placement_uses_the_phone_plate_and_caption() {
    let lines: Vec<LineInput> = (0..57)
        .map(|i| LineInput {
            short: "DMARC".to_string(),
            group: 0,
            tone: if i == 20 { LineTone::Fail } else { LineTone::Pass },
        })
        .collect();
    let p = place_phone(&lines).unwrap();
    assert_eq!(phone_plate(), 358.0);
    assert_eq!(p.plate_width, 358.0);
    assert!((p.pitch - 358.0 / 57.0).abs() < 1e-9, "pitch {}", p.pitch);
    // 5 characters x 0.62 em x 10 px + 4 px padding.
    assert!((phone_label_width(5) - 35.0).abs() < 1e-9);
    let label = &p.labels[0];
    assert!((label.width - 35.0).abs() < 1e-9);
    assert!((label.leader - 20.5 * 358.0 / 57.0).abs() < 1e-9);
    assert_eq!(label.start, label.leader);
}

#[test]
fn c2_every_run_carries_its_phone_placement() {
    for (name, runs) in pages() {
        let inputs: Vec<RunInput> = runs.iter().map(layout_input_or_current).collect();
        let l = layout(&inputs[0], &inputs[1..], catalogue(), W).unwrap();
        for (k, run) in l.runs.iter().enumerate() {
            let lines: Vec<LineInput> = l
                .lines
                .iter()
                .zip(&run.tones)
                .map(|(line, tone)| LineInput {
                    short: line.short.clone(),
                    group: line.protocol as usize,
                    tone: *tone,
                })
                .collect();
            let p = place_phone(&lines).unwrap();
            assert_eq!(run.phone_labels, p.labels, "{name} run {k}");
            assert_eq!(run.phone_rows, p.rows, "{name} run {k}");
        }
    }
}

/// A desktop label as the phone draws it: leader scaled to the phone plate, box measured at the
/// phone caption, start clamped into the plate as the stylesheet does.
fn drawn_at_phone(l: &Label, chars: usize) -> Label {
    let leader = l.leader / f64::from(W) * phone_plate();
    let width = phone_label_width(chars);
    Label {
        leader,
        width,
        start: leader.min(phone_plate() - width).max(0.0),
        ..l.clone()
    }
}

#[test]
fn c3_fixture_runs_clear_at_the_desktop_are_clear_at_the_phone() {
    let (mut clear, mut dense) = (0, 0);
    for (name, runs) in pages() {
        let inputs: Vec<RunInput> = runs.iter().map(layout_input_or_current).collect();
        let l = layout(&inputs[0], &inputs[1..], catalogue(), W).unwrap();
        for (k, run) in l.runs.iter().enumerate() {
            let phone = crossings(&run.phone_labels);
            let desktop = crossings(&run.labels);
            assert_eq!(phone.is_empty(), desktop.is_empty(), "{name} run {k}: {phone:?}");
            if phone.is_empty() {
                clear += 1;
            } else {
                // A dense cluster falls back to the line-order placement at both widths.
                dense += 1;
            }
        }
    }
    // example.com and stale.example.com hold twelve adjacent HTTP labels: a dense cluster.
    assert_eq!((clear, dense), (5, 2));
}

/// HSTS, XFO and XCTO warn: three adjacent HTTP lines.
fn three_headers() -> RunInput<'static> {
    let mut run = layout_input_or_current(&fixture("new.example.com.json"));
    for status in run.statuses.values_mut() {
        if matches!(status, Status::Warn | Status::Fail) {
            *status = Status::Pass;
        }
    }
    for id in ["http.hsts", "http.x_frame_options", "http.x_content_type_options"] {
        run.statuses.insert(CheckId::parse(id).unwrap(), Status::Warn);
    }
    run
}

#[test]
fn c7_the_desktop_placement_drawn_at_the_phone_crosses_where_the_phone_placement_does_not() {
    let l = layout(&three_headers(), &[], catalogue(), W).unwrap();
    let run = &l.runs[0];
    assert!(crossings(&run.labels).is_empty(), "{:?}", run.labels);
    let drawn: Vec<Label> = run
        .labels
        .iter()
        .map(|l| drawn_at_phone(l, l.text.chars().count()))
        .collect();
    assert_eq!(crossings(&drawn), ["HSTS (row 0) covers XFO (row 1)"]);
    assert!(crossings(&run.phone_labels).is_empty(), "{:?}", run.phone_labels);
    assert_eq!(run.phone_labels.len(), 3);
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let i = tag.find(&needle)? + needle.len();
    let end = tag[i..].find('"')? + i;
    Some(&tag[i..end])
}

fn tags<'a>(html: &'a str, class: &str) -> Vec<&'a str> {
    let needle = format!("class=\"{class}\"");
    html.match_indices(&needle)
        .map(|(i, _)| {
            let start = html[..i].rfind('<').unwrap();
            let end = html[i..].find('>').unwrap() + i;
            &html[start..end]
        })
        .collect()
}

fn style_value<'a>(tag: &'a str, prop: &str) -> Option<&'a str> {
    let style = attr(tag, "style")?;
    let needle = format!("{prop}: ");
    let i = style.find(&needle)? + needle.len();
    let rest = &style[i..];
    Some(rest[..rest.find(';').unwrap_or(rest.len())].trim())
}

#[test]
fn c4_the_component_draws_the_phone_placement() {
    let runs = vec![fixture("example.com.json")];
    let inputs: Vec<RunInput> = runs.iter().map(layout_input_or_current).collect();
    let l = layout(&inputs[0], &[], catalogue(), W).unwrap();
    let html = spectrum(&SpectrumView::from_layout(&l, "Checks", 0)).unwrap();

    let extra = l.runs[0].phone_rows.saturating_sub(3).to_string();
    let root = tags(&html, "spectrum")[0];
    assert_eq!(style_value(root, "--label-extra-phone"), Some(extra.as_str()));

    let labels = tags(&html, "spectrum__line-label");
    let leaders = tags(&html, "spectrum__leader");
    assert_eq!(labels.len(), l.runs[0].phone_labels.len());
    for ((label, leader), phone) in labels.iter().zip(&leaders).zip(&l.runs[0].phone_labels) {
        let code = l.lines[phone.line].id.to_string();
        assert_eq!(attr(label, "data-code"), Some(code.as_str()));
        let row = phone.row.to_string();
        assert_eq!(style_value(label, "--row-phone"), Some(row.as_str()), "{label}");
        assert_eq!(style_value(leader, "--row-phone"), Some(row.as_str()), "{leader}");
        let width = format!("{:.2}px", phone.width);
        assert_eq!(style_value(label, "--width-phone"), Some(width.as_str()));
        let x = format!("{:.4}%", phone.leader / phone_plate() * 100.0);
        assert_eq!(style_value(label, "--x"), Some(x.as_str()));
    }
}

#[test]
fn c5_the_slim_band_carries_no_phone_rows() {
    let runs = vec![fixture("example.com.json")];
    let inputs: Vec<RunInput> = runs.iter().map(layout_input_or_current).collect();
    let l = layout(&inputs[0], &[], catalogue(), W).unwrap();
    let html = netray_web::components::spectrum_slim(&SpectrumView::from_layout(&l, "Checks", 0))
        .unwrap();
    assert!(html.contains("<div class=\"spectrum\" data-slim>"), "{html}");
}

/// The rules of the phone media query (`max-width: 480px`) in `spectrum.css`.
fn phone_rules() -> String {
    let css = include_str!("../../static/spectrum.css");
    let start = css.find("@media (max-width: 480px)").expect("phone media query");
    css[start..].to_string()
}

fn rule_body(css: &str, selector: &str) -> String {
    let mut rest = css;
    let mut body = String::new();
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}').unwrap() + open;
        let sel = rest[..open].rsplit('{').next().unwrap();
        if sel.split(',').any(|s| s.trim() == selector) {
            body.push_str(&rest[open + 1..close]);
        }
        rest = &rest[close + 1..];
    }
    body
}

#[test]
fn c6_the_phone_stylesheet_reads_the_phone_placement() {
    let css = phone_rules();
    let label = rule_body(&css, ".spectrum__line-label");
    assert!(
        label.contains(
            "bottom: calc(var(--spectrum-label-leader) + var(--row-phone) * var(--spectrum-label-row))"
        ),
        "{label}"
    );
    assert!(
        label.contains("left: clamp(0px, var(--x), calc(100% - var(--width-phone)))"),
        "{label}"
    );
    let leader = rule_body(&css, ".spectrum__leader");
    assert!(
        leader.contains(
            "height: calc(var(--spectrum-label-leader) + var(--row-phone) * var(--spectrum-label-row))"
        ),
        "{leader}"
    );
    let band = rule_body(&css, ".spectrum__label-band");
    assert!(
        band.contains(
            "height: calc(var(--spectrum-label-band) + var(--label-extra-phone) * var(--spectrum-label-row))"
        ),
        "{band}"
    );
}

