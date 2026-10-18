//! The OG card's plate (V2 spec v2-result-page, Phase 7, requirement 29): the card draws the
//! page's layout of the stored run, so line positions and tones agree with the page.
use std::collections::BTreeSet;

use netray_engine::catalogue::catalogue;
use netray_model::run::{Run, RunCheck};
use netray_model::{Grade, Status};
use netray_web::pages::result::views::layout_input_or_current;
use netray_web::plate::{CARD_PLATE_WIDTH, card_layout, card_plate};
use netray_web::spectrum::{LineTone, layout, line_pitch_min};

fn run_with(status_of: impl Fn(usize, &str) -> Status) -> Run {
    let checks = catalogue()
        .entries()
        .iter()
        .enumerate()
        .map(|(i, e)| RunCheck {
            id: e.id.clone(),
            status: status_of(i, &e.id.to_string()),
            findings: Vec::new(),
            evidence: Vec::new(),
        })
        .collect();
    Run {
        profile_version: 3,
        engine_version: "0.0.0".into(),
        evidence_schema_version: 1,
        grade: Grade::B,
        complete: true,
        sections: Vec::new(),
        checks,
        blocks: Vec::new(),
    }
}

fn mixed() -> Run {
    run_with(|i, _| [Status::Pass, Status::Warn, Status::Fail][i % 3])
}

/// The `data-code` values of the elements carrying `class="<class>"`.
fn codes(svg: &str, class: &str) -> Vec<String> {
    let needle = format!("class=\"{class}\"");
    svg.match_indices(&needle)
        .map(|(at, _)| {
            let start = svg[..at].rfind('<').expect("open tag");
            let end = at + svg[at..].find('>').expect("close tag");
            let tag = &svg[start..end];
            let d = tag.find("data-code=\"").expect("data-code") + "data-code=\"".len();
            tag[d..d + tag[d..].find('"').unwrap()].to_string()
        })
        .collect()
}

fn tag_of<'a>(svg: &'a str, code: &str) -> &'a str {
    let needle = format!("data-code=\"{code}\"");
    let line = svg
        .match_indices("class=\"plate__line\"")
        .map(|(at, _)| {
            let start = svg[..at].rfind('<').unwrap();
            &svg[start..at + svg[at..].find('>').unwrap()]
        })
        .find(|t| t.contains(&needle))
        .expect("line");
    line
}

#[test]
fn c2_plate_holds_one_line_per_catalogue_id_and_labels_exactly_warn_and_fail() {
    let run = mixed();
    let svg = card_plate(&card_layout(&run, None));

    let lines = codes(&svg, "plate__line");
    assert_eq!(lines.len(), 54);
    let ids: BTreeSet<String> = catalogue()
        .entries()
        .iter()
        .map(|e| e.id.to_string())
        .collect();
    assert_eq!(lines.iter().cloned().collect::<BTreeSet<_>>(), ids);

    let labels = codes(&svg, "plate__label");
    let want: BTreeSet<String> = run
        .checks
        .iter()
        .filter(|c| matches!(c.status, Status::Warn | Status::Fail))
        .map(|c| c.id.to_string())
        .collect();
    assert_eq!(labels.len(), want.len());
    assert_eq!(labels.into_iter().collect::<BTreeSet<_>>(), want);
}

#[test]
fn c3_a_failed_predecessor_check_now_passing_is_fixed_and_labelled_unless_swept() {
    let pred = run_with(|_, id| {
        if id == "http.hsts" {
            Status::Fail
        } else {
            Status::Pass
        }
    });
    let run = run_with(|_, _| Status::Pass);

    let svg = card_plate(&card_layout(&run, Some(&pred)));
    assert!(tag_of(&svg, "http.hsts").contains("data-tone=\"fixed\""));
    assert!(codes(&svg, "plate__label").contains(&"http.hsts".to_string()));

    let swept = card_plate(&card_layout(&run, None));
    assert!(!swept.contains("data-tone=\"fixed\""));
}

#[test]
fn c4_card_layout_matches_the_pages_layout() {
    let run = mixed();
    let width = (catalogue().entries().len() as f64 * line_pitch_min()) as u32;
    let page = layout(&layout_input_or_current(&run), &[], catalogue(), width).expect("page");
    let card = card_layout(&run, None);

    assert_eq!(CARD_PLATE_WIDTH, 1104);
    assert_eq!(page.lines.len(), card.lines.len());
    for (p, c) in page.lines.iter().zip(&card.lines) {
        assert_eq!(p.id, c.id);
        let (px, cx) = (p.x / page.plate_width, c.x / card.plate_width);
        assert!((px - cx).abs() < 1e-9, "{}: {px} vs {cx}", p.id);
    }
    let tones: &Vec<LineTone> = &page.runs[0].tones;
    assert_eq!(tones, &card.runs[0].tones);
}
