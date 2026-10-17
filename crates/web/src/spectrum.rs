//! The spectrum layout: plate width, line positions, groups and label rows as pure functions of
//! the lines and a container width (SDD `specs/sdd/v2.md` §5.4 and §5.5; feature spec
//! `specs/features/v2-spectrum/spec.md`, requirements 1-8). Every length is in px.

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
