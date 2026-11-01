//! The OG card's plate (spec v2-result-page, Phase 7, requirement 29): the page's spectrum
//! layout of a stored run drawn as one SVG group in the light theme's literal colours, since the
//! card's renderer has no CSS variables. Pure; lens places the group with a transform.

use std::fmt::Write;

use netray_engine::catalogue::catalogue;
use netray_engine::domain_run::grade_label;
use netray_engine::history::score_run;
use netray_engine::profile::Profiles;
use netray_engine::scoring::grade_for;
use netray_model::run::Run;

use crate::og::layout::{GRADES_FONT_SIZE, PLATE_X, PLATE_Y, RIGHT_X, SCORE_BASELINE_Y};
use crate::pages::result::views::{layout_input_or_current, section_name};
use crate::spectrum::{self, Layout, LineTone, px};
use crate::tokens::values::{
    COLOR_LIGHT_BG, COLOR_LIGHT_GRID, COLOR_LIGHT_INK, COLOR_LIGHT_PLATE, COLOR_LIGHT_PLATE_BORDER,
    FONT_SIZE_LABEL, LINE_FAIL, LINE_HAIR, LINE_PASS, LINE_WARN, SPECTRUM_LABEL_LEADER,
    SPECTRUM_LABEL_ROW, STATUS_LIGHT_FAIL_MARK, STATUS_LIGHT_FAIL_TEXT, STATUS_LIGHT_FIX_MARK,
    STATUS_LIGHT_FIX_TEXT, STATUS_LIGHT_PASS_MARK, STATUS_LIGHT_UNTESTED_MARK,
    STATUS_LIGHT_WARN_MARK, STATUS_LIGHT_WARN_TEXT,
};

/// The card's plate width in px (requirement 29).
pub const CARD_PLATE_WIDTH: u32 = 1104;
/// The card's plate height in px; the page's is 92.
pub const CARD_PLATE_HEIGHT: f64 = 56.0;

const LABEL_PAD: f64 = 4.0;
const RING_RADIUS: f64 = 3.5;
const LABEL_BASELINE_GAP: f64 = 3.0;

/// The layout of a stored run and its predecessor (none when swept, so no line is fixed) at the
/// card's plate width.
pub fn card_layout(run: &Run, predecessor: Option<&Run>) -> Layout {
    let current = layout_input_or_current(run);
    let older: Vec<_> = predecessor
        .map(layout_input_or_current)
        .into_iter()
        .collect();
    let mut layout = spectrum::layout(&current, &older, catalogue(), CARD_PLATE_WIDTH)
        .expect("the catalogue has lines");
    let profile = Profiles::get(run.profile_version).unwrap_or_else(Profiles::current);
    layout.grades = score_run(run, profile)
        .sections
        .iter()
        .map(|(protocol, section)| (*protocol, grade_for(profile, section.percentage)))
        .collect();
    layout
}

fn tone_name(tone: LineTone) -> &'static str {
    match tone {
        LineTone::Pass => "pass",
        LineTone::Warn => "warn",
        LineTone::Fail => "fail",
        LineTone::Fixed => "fixed",
        LineTone::Muted => "muted",
    }
}

fn mark(tone: LineTone) -> (&'static str, &'static str) {
    match tone {
        LineTone::Pass => (STATUS_LIGHT_PASS_MARK, LINE_PASS),
        LineTone::Warn => (STATUS_LIGHT_WARN_MARK, LINE_WARN),
        LineTone::Fail => (STATUS_LIGHT_FAIL_MARK, LINE_FAIL),
        LineTone::Fixed => (STATUS_LIGHT_FIX_MARK, LINE_PASS),
        LineTone::Muted => (STATUS_LIGHT_UNTESTED_MARK, LINE_HAIR),
    }
}

fn text_colour(tone: LineTone) -> &'static str {
    match tone {
        LineTone::Fail => STATUS_LIGHT_FAIL_TEXT,
        LineTone::Fixed => STATUS_LIGHT_FIX_TEXT,
        _ => STATUS_LIGHT_WARN_TEXT,
    }
}

/// The SVG plate of the layout's run 0 as one `<g class="plate">` on a single line, its origin
/// the plate's top left; labels rise above it.
pub fn card_plate(layout: &Layout) -> String {
    let run = &layout.runs[0];
    let width = layout.plate_width;
    let height = CARD_PLATE_HEIGHT;
    let leader = px(SPECTRUM_LABEL_LEADER);
    let row = px(SPECTRUM_LABEL_ROW);
    let label_size = px(FONT_SIZE_LABEL);

    let mut out = String::from("<g class=\"plate\">");
    let _ = write!(
        out,
        "<rect class=\"plate__frame\" x=\"0\" y=\"0\" width=\"{width:.2}\" height=\"{height:.2}\" fill=\"{COLOR_LIGHT_PLATE}\" stroke=\"{COLOR_LIGHT_PLATE_BORDER}\" stroke-width=\"{}\"/>",
        px(LINE_HAIR)
    );
    for b in &layout.boundaries {
        let _ = write!(
            out,
            "<line class=\"plate__boundary\" x1=\"{b:.2}\" x2=\"{b:.2}\" y1=\"0\" y2=\"{height:.2}\" stroke=\"{COLOR_LIGHT_GRID}\" stroke-width=\"{}\"/>",
            px(LINE_HAIR)
        );
    }
    for (i, line) in layout.lines.iter().enumerate() {
        let tone = run.tones[i];
        let (colour, stroke) = mark(tone);
        let top = height * (1.0 - run.heights[i]);
        if tone == LineTone::Fixed {
            let _ = write!(
                out,
                "<g class=\"plate__line\" data-code=\"{}\" data-tone=\"fixed\"><rect class=\"plate__outline\" x=\"{:.2}\" y=\"{top:.2}\" width=\"5\" height=\"{:.2}\" fill=\"none\" stroke=\"{colour}\" stroke-width=\"{}\"/><circle class=\"plate__ring\" cx=\"{:.2}\" cy=\"{top:.2}\" r=\"{RING_RADIUS}\" fill=\"none\" stroke=\"{colour}\" stroke-width=\"{}\"/></g>",
                line.id,
                line.x - 2.5,
                height - top,
                px(LINE_HAIR),
                line.x,
                px(LINE_HAIR)
            );
        } else {
            let _ = write!(
                out,
                "<line class=\"plate__line\" data-code=\"{}\" data-tone=\"{}\" x1=\"{x:.2}\" x2=\"{x:.2}\" y1=\"{height:.2}\" y2=\"{top:.2}\" stroke=\"{colour}\" stroke-width=\"{}\"/>",
                line.id,
                tone_name(tone),
                px(stroke),
                x = line.x
            );
        }
    }
    for label in &run.labels {
        let line = &layout.lines[label.line];
        let tone = run.tones[label.line];
        let rise = leader + label.row as f64 * row;
        let _ = write!(
            out,
            "<line class=\"plate__leader\" x1=\"{x:.2}\" x2=\"{x:.2}\" y1=\"0\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{}\"/>",
            -rise,
            mark(tone).0,
            px(LINE_HAIR),
            x = line.x
        );
    }
    for label in &run.labels {
        let line = &layout.lines[label.line];
        let tone = run.tones[label.line];
        let rise = leader + label.row as f64 * row;
        let _ = write!(
            out,
            "<rect class=\"plate__backing\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{label_size:.2}\" fill=\"{COLOR_LIGHT_BG}\"/>",
            label.start,
            -rise - label_size,
            label.width
        );
        let _ = write!(
            out,
            "<text class=\"plate__label\" data-code=\"{}\" data-tone=\"{}\" x=\"{:.2}\" y=\"{:.2}\" font-family=\"JetBrains Mono\" font-size=\"{label_size}\" fill=\"{}\">{}</text>",
            line.id,
            tone_name(tone),
            label.start + LABEL_PAD,
            -rise - LABEL_BASELINE_GAP,
            text_colour(tone),
            escape(&label.text)
        );
    }
    if !layout.grades.is_empty() {
        let grades = layout
            .grades
            .iter()
            .map(|(protocol, grade)| format!("{} {}", section_name(*protocol), grade_label(*grade)))
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        let _ = write!(
            out,
            "<text class=\"plate__grades\" x=\"{}\" y=\"{}\" font-family=\"Inter\" font-size=\"{GRADES_FONT_SIZE}\" font-weight=\"700\" fill=\"{COLOR_LIGHT_INK}\">{}</text>",
            RIGHT_X - PLATE_X,
            SCORE_BASELINE_Y - PLATE_Y,
            escape(&grades)
        );
    }
    out.push_str("</g>");
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use netray_model::run::RunCheck;
    use netray_model::{Grade, Status};

    fn all_fail() -> Run {
        Run {
            profile_version: 3,
            engine_version: "0.0.0".into(),
            evidence_schema_version: 1,
            grade: Grade::F,
            complete: true,
            sections: Vec::new(),
            checks: catalogue()
                .entries()
                .iter()
                .map(|e| RunCheck {
                    id: e.id.clone(),
                    status: Status::Fail,
                    findings: Vec::new(),
                    evidence: Vec::new(),
                })
                .collect(),
            blocks: Vec::new(),
        }
    }

    #[test]
    fn an_all_fail_run_has_at_most_four_label_rows() {
        let layout = card_layout(&all_fail(), None);
        assert!(layout.runs[0].rows <= 4, "{} rows", layout.runs[0].rows);
        assert!(!card_plate(&layout).contains('\n'));
    }
}
