//! The spectrum layout: plate width, line positions, groups and label rows as pure functions of
//! the lines and a container width (SDD `specs/sdd/v2.md` §5.4 and §5.5; feature spec
//! `specs/features/v2-spectrum/spec.md`, requirements 1-12). Every length is in px.

use std::collections::BTreeMap;

use netray_engine::catalogue::{Catalogue, CheckSpec};
use netray_engine::profile::Profile;
use netray_model::{CheckId, Protocol, Status};

use crate::tokens::values::{FONT_SIZE_LABEL, SPACE_1, SPECTRUM_LINE_PITCH_MIN};

const CHAR_WIDTH_EM: f64 = 0.62;
const TOLERANCE: f64 = 1e-6;
const MIN_ROWS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineTone {
    Pass,
    Warn,
    Fail,
    Fixed,
    Muted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineInput {
    pub short: String,
    pub group: usize,
    pub tone: LineTone,
}

/// A maximal run of consecutive lines with equal `group` value.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub first: usize,
    pub len: usize,
    pub start: f64,
    pub width: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub line: usize,
    pub text: String,
    pub row: usize,
    pub start: f64,
    pub width: f64,
    pub leader: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub plate_width: f64,
    pub pitch: f64,
    pub xs: Vec<f64>,
    pub groups: Vec<Group>,
    pub boundaries: Vec<f64>,
    pub labels: Vec<Label>,
    pub rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    NoLines,
}

fn px(value: &str) -> f64 {
    value
        .strip_suffix("px")
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("token value {value:?} is not a px length"))
}

/// `spectrum.linePitchMin` in px.
pub fn line_pitch_min() -> f64 {
    px(SPECTRUM_LINE_PITCH_MIN)
}

/// Width of a label box of `chars` characters.
pub fn label_width(chars: usize) -> f64 {
    chars as f64 * CHAR_WIDTH_EM * px(FONT_SIZE_LABEL) + px(SPACE_1)
}

fn overlaps(a: &Label, b: &Label) -> bool {
    a.start < b.start + b.width - TOLERANCE && b.start < a.start + a.width - TOLERANCE
}

/// The box start at a wider plate `to_plate` for a placement made at `plate`: equals `label.start`
/// at `plate`, keeps rows free of overlap, keeps a box no wider than `to_plate` inside it and over
/// its scaled leader (SDD §5.5 point 2).
pub fn scaled_start(label: &Label, plate: f64, to_plate: f64) -> f64 {
    let r = to_plate / plate;
    (r * label.leader).min(to_plate - label.width).max(0.0)
}

/// The placement for `lines` in a container `width` px wide.
pub fn place(lines: &[LineInput], width: u32) -> Result<Placement, LayoutError> {
    let n = lines.len();
    if n == 0 {
        return Err(LayoutError::NoLines);
    }
    let min = line_pitch_min();
    let needed = n as f64 * min;
    let (plate_width, pitch) = if f64::from(width) >= needed {
        (f64::from(width), f64::from(width) / n as f64)
    } else {
        (needed, min)
    };

    let xs: Vec<f64> = (0..n).map(|i| (i as f64 + 0.5) * pitch).collect();

    let mut groups: Vec<Group> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 && lines[i - 1].group == line.group {
            if let Some(g) = groups.last_mut() {
                g.len += 1;
                g.width = g.len as f64 * pitch;
            }
        } else {
            groups.push(Group {
                first: i,
                len: 1,
                start: i as f64 * pitch,
                width: pitch,
            });
        }
    }
    let boundaries = groups.iter().skip(1).map(|g| g.start).collect();

    let mut labels: Vec<Label> = Vec::new();
    let mut highest = 0;
    for (i, line) in lines.iter().enumerate() {
        if !matches!(line.tone, LineTone::Warn | LineTone::Fail | LineTone::Fixed) {
            continue;
        }
        let w = label_width(line.short.chars().count());
        let x = xs[i];
        let start = if w > plate_width {
            0.0
        } else if x + w > plate_width {
            plate_width - w
        } else {
            x
        };
        let mut label = Label {
            line: i,
            text: line.short.clone(),
            row: 0,
            start,
            width: w,
            leader: x,
        };
        while labels
            .iter()
            .any(|l| l.row == label.row && overlaps(l, &label))
        {
            label.row += 1;
        }
        highest = highest.max(label.row);
        labels.push(label);
    }

    Ok(Placement {
        plate_width,
        pitch,
        xs,
        groups,
        boundaries,
        labels,
        rows: MIN_ROWS.max(highest + 1),
    })
}

/// Whether a protocol's section was measured in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionState {
    Measured,
    NotApplicable,
    Unmeasured,
}

/// One run as the layout reads it. A protocol absent from `sections` is Measured; a catalogue
/// check absent from `statuses` is muted.
#[derive(Debug, Clone)]
pub struct RunInput<'a> {
    pub statuses: BTreeMap<CheckId, Status>,
    pub sections: BTreeMap<Protocol, SectionState>,
    pub profile: &'a Profile,
}

/// One spectrum line: a catalogue check at its shared x position.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutLine {
    pub id: CheckId,
    pub protocol: Protocol,
    pub short: String,
    pub x: f64,
}

/// What one run draws: a tone and a height per line, its label placement and its section states.
#[derive(Debug, Clone, PartialEq)]
pub struct RunLayout {
    pub tones: Vec<LineTone>,
    pub heights: Vec<f64>,
    pub labels: Vec<Label>,
    pub rows: usize,
    pub sections: BTreeMap<Protocol, SectionState>,
}

/// The shared geometry plus one `RunLayout` per run, current first, then the older runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub plate_width: f64,
    pub pitch: f64,
    pub lines: Vec<LayoutLine>,
    pub groups: Vec<Group>,
    pub boundaries: Vec<f64>,
    pub runs: Vec<RunLayout>,
}

/// The effective state of a protocol's section in a run: the supplied state (absent is Measured),
/// except that a supplied Measured becomes Unmeasured when any check the profile weights in that
/// protocol has status Unmeasured.
fn section_state(run: &RunInput, protocol: Protocol) -> SectionState {
    let supplied = run
        .sections
        .get(&protocol)
        .copied()
        .unwrap_or(SectionState::Measured);
    let unmeasured = run.profile.sections.get(&protocol).is_some_and(|s| {
        s.checks
            .keys()
            .any(|id| run.statuses.get(id) == Some(&Status::Unmeasured))
    });
    if supplied == SectionState::Measured && unmeasured {
        SectionState::Unmeasured
    } else {
        supplied
    }
}

/// The status a line shows in a run, or `None` when the line is muted there.
fn measured(run: &RunInput, spec: &CheckSpec) -> Option<Status> {
    let status = run.statuses.get(&spec.id).copied()?;
    let weighted = run
        .profile
        .sections
        .get(&spec.protocol)
        .is_some_and(|s| s.checks.contains_key(&spec.id));
    let counts = matches!(status, Status::Pass | Status::Warn | Status::Fail)
        && weighted
        && section_state(run, spec.protocol) == SectionState::Measured;
    counts.then_some(status)
}

fn base_tone(status: Option<Status>) -> LineTone {
    match status {
        Some(Status::Pass) => LineTone::Pass,
        Some(Status::Warn) => LineTone::Warn,
        Some(Status::Fail) => LineTone::Fail,
        _ => LineTone::Muted,
    }
}

/// Heights in (0, 1]: a measured line's share of its section weight, scaled to the run's largest;
/// muted lines take the smallest measured height.
fn run_heights(run: &RunInput, specs: &[&CheckSpec], tones: &[LineTone]) -> Vec<f64> {
    let weight = |spec: &CheckSpec| -> f64 {
        run.profile
            .sections
            .get(&spec.protocol)
            .and_then(|s| s.checks.get(&spec.id))
            .map_or(0.0, |w| f64::from(*w))
    };
    let totals: BTreeMap<Protocol, f64> = run
        .profile
        .sections
        .iter()
        .filter(|(protocol, _)| section_state(run, **protocol) == SectionState::Measured)
        .map(|(protocol, section)| {
            let total = section
                .checks
                .iter()
                .filter(|(id, _)| {
                    matches!(
                        run.statuses.get(*id),
                        Some(Status::Pass | Status::Warn | Status::Fail)
                    )
                })
                .map(|(_, w)| f64::from(*w))
                .sum();
            (*protocol, total)
        })
        .collect();
    let shares: Vec<Option<f64>> = specs
        .iter()
        .zip(tones)
        .map(|(spec, tone)| {
            if *tone == LineTone::Muted {
                return None;
            }
            let section = run
                .profile
                .sections
                .get(&spec.protocol)
                .map_or(0.0, |s| f64::from(s.weight));
            let total = totals.get(&spec.protocol).copied().unwrap_or(0.0);
            Some(weight(spec) / total * section)
        })
        .collect();
    let largest = shares.iter().flatten().copied().fold(0.0, f64::max);
    if largest <= 0.0 {
        return vec![1.0; specs.len()];
    }
    let scaled: Vec<Option<f64>> = shares.iter().map(|e| e.map(|e| e / largest)).collect();
    let smallest = scaled
        .iter()
        .flatten()
        .copied()
        .fold(f64::INFINITY, f64::min);
    scaled.iter().map(|h| h.unwrap_or(smallest)).collect()
}

/// The layout of `current` and `older` (newest first) over the catalogue's lines in a container
/// `width` px wide. Geometry comes from the current run and is shared by all runs.
pub fn layout(
    current: &RunInput,
    older: &[RunInput],
    catalogue: &Catalogue,
    width: u32,
) -> Result<Layout, LayoutError> {
    let mut specs: Vec<&CheckSpec> = catalogue.entries().iter().collect();
    specs.sort_by_key(|s| s.protocol);

    let mut inputs: Vec<&RunInput> = vec![current];
    inputs.extend(older);
    let bases: Vec<Vec<LineTone>> = inputs
        .iter()
        .map(|run| {
            specs
                .iter()
                .map(|spec| base_tone(measured(run, spec)))
                .collect()
        })
        .collect();

    let mut runs = Vec::with_capacity(inputs.len());
    let mut geometry: Option<Placement> = None;
    for (k, run) in inputs.iter().enumerate() {
        let tones: Vec<LineTone> = bases[k]
            .iter()
            .enumerate()
            .map(|(i, tone)| {
                let regressed = bases
                    .get(k + 1)
                    .is_some_and(|prev| matches!(prev[i], LineTone::Warn | LineTone::Fail));
                if *tone == LineTone::Pass && regressed {
                    LineTone::Fixed
                } else {
                    *tone
                }
            })
            .collect();
        let line_inputs: Vec<LineInput> = specs
            .iter()
            .zip(&tones)
            .map(|(spec, tone)| LineInput {
                short: spec.short.clone(),
                group: spec.protocol as usize,
                tone: *tone,
            })
            .collect();
        let placement = place(&line_inputs, width)?;
        let sections = specs
            .iter()
            .map(|spec| (spec.protocol, section_state(run, spec.protocol)))
            .collect();
        runs.push(RunLayout {
            heights: run_heights(run, &specs, &tones),
            tones,
            labels: placement.labels.clone(),
            rows: placement.rows,
            sections,
        });
        geometry.get_or_insert(placement);
    }

    let geometry = geometry.ok_or(LayoutError::NoLines)?;
    let lines = specs
        .iter()
        .zip(&geometry.xs)
        .map(|(spec, x)| LayoutLine {
            id: spec.id.clone(),
            protocol: spec.protocol,
            short: spec.short.clone(),
            x: *x,
        })
        .collect();
    Ok(Layout {
        plate_width: geometry.plate_width,
        pitch: geometry.pitch,
        lines,
        groups: geometry.groups,
        boundaries: geometry.boundaries,
        runs,
    })
}
