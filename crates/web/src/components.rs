//! The ten R44 components of the V2 design system. Each is an askama macro under
//! `templates/components/` and a stylesheet under `static/`; the functions here render a macro
//! from a view. Styling lives in CSS keyed on classes and `data-*` attributes, never in markup.

use askama::Template;

use crate::evidence::Badge;

/// The visual tone of a mark; not a check status, which belongs to `netray-model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Pass,
    Warn,
    Fail,
    Fix,
    Untested,
}

impl Tone {
    /// The `data-tone` value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Tone::Pass => "pass",
            Tone::Warn => "warn",
            Tone::Fail => "fail",
            Tone::Fix => "fix",
            Tone::Untested => "untested",
        }
    }
}

/// The state of one progress-ray segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentState {
    Pending,
    Done,
    Incomplete,
}

impl SegmentState {
    /// The `data-state` value.
    pub fn as_str(&self) -> &'static str {
        match self {
            SegmentState::Pending => "pending",
            SegmentState::Done => "done",
            SegmentState::Incomplete => "incomplete",
        }
    }
}

/// One value of an evidence block: its text, the inspector page it links to and its badges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cell {
    pub text: String,
    pub href: Option<String>,
    pub badges: Vec<Badge>,
}

impl From<String> for Cell {
    fn from(text: String) -> Self {
        Cell {
            text,
            ..Cell::default()
        }
    }
}

impl From<&str> for Cell {
    fn from(text: &str) -> Self {
        Cell::from(text.to_string())
    }
}

/// A DNS record row of an evidence block.
#[derive(Debug, Clone)]
pub struct RecordRow {
    pub owner: Cell,
    pub rtype: Cell,
    pub ttl: Cell,
    pub data: Cell,
}

/// A key and its value.
#[derive(Debug, Clone)]
pub struct Field {
    pub key: String,
    pub value: Cell,
}

/// A table column header; `key` marks the column whose value identifies a row.
#[derive(Debug, Clone)]
pub struct TableColumn {
    pub name: String,
    pub key: bool,
}

impl From<&str> for TableColumn {
    fn from(name: &str) -> Self {
        TableColumn {
            name: name.to_string(),
            key: false,
        }
    }
}

/// The deepest nesting level the stylesheet has a rule for; deeper nodes render at this level.
pub const MAX_TREE_DEPTH: u8 = 7;

/// One node of a flattened tree; `depth` is its nesting level.
#[derive(Debug, Clone)]
pub struct TreeItem {
    pub depth: u8,
    pub text: String,
}

impl TreeItem {
    /// The `data-depth` value: `depth` clamped to [`MAX_TREE_DEPTH`].
    pub fn depth_attr(&self) -> u8 {
        self.depth.min(MAX_TREE_DEPTH)
    }
}

/// The shape of an evidence block's content.
#[derive(Debug, Clone)]
pub enum EvidenceShape {
    Records(Vec<RecordRow>),
    Table {
        columns: Vec<TableColumn>,
        rows: Vec<Vec<Cell>>,
    },
    Chain(Vec<Vec<Field>>),
    Tree(Vec<TreeItem>),
    Fields(Vec<Field>),
    Text(String),
}

impl EvidenceShape {
    /// The `data-shape` value.
    pub fn as_str(&self) -> &'static str {
        match self {
            EvidenceShape::Records(_) => "records",
            EvidenceShape::Table { .. } => "table",
            EvidenceShape::Chain(_) => "chain",
            EvidenceShape::Tree(_) => "tree",
            EvidenceShape::Fields(_) => "fields",
            EvidenceShape::Text(_) => "text",
        }
    }
}

/// A letter grade with an optional score out of 100.
#[derive(Debug, Clone)]
pub struct GradeView {
    pub letter: String,
    pub score: Option<u8>,
    pub tone: Tone,
}

impl GradeView {
    /// The grade legend's "descriptor, meaning" of the letter.
    pub fn descriptor(&self) -> &'static str {
        use netray_model::Grade;
        crate::grade_legend::descriptor(match self.letter.as_str() {
            "A+" => Grade::APlus,
            "A" => Grade::A,
            "B" => Grade::B,
            "C" => Grade::C,
            "D" => Grade::D,
            "F" => Grade::F,
            _ => Grade::Incomplete,
        })
    }
}

/// The run line: parts joined by a separator.
#[derive(Debug, Clone)]
pub struct RunLineView {
    pub parts: Vec<String>,
}

/// One segment of the progress ray.
#[derive(Debug, Clone)]
pub struct RaySegment {
    pub label: String,
    pub state: SegmentState,
}

/// The progress ray.
#[derive(Debug, Clone)]
pub struct ProgressRayView {
    pub label: String,
    pub segments: Vec<RaySegment>,
}

/// A spectrum line in one run; `x` and `height` are percent of the plate.
#[derive(Debug, Clone)]
pub struct SpectrumLine {
    pub code: String,
    pub x: f64,
    pub height: f64,
    pub tone: Tone,
}

impl SpectrumLine {
    fn top(&self) -> f64 {
        100.0 - self.height
    }

    fn x_pct(&self) -> String {
        format!("{:.4}", self.x)
    }

    fn top_pct(&self) -> String {
        format!("{:.4}", self.top())
    }

    fn height_pct(&self) -> String {
        format!("{:.4}", self.height)
    }

    fn is_fix(&self) -> bool {
        self.tone == Tone::Fix
    }
}

/// One run's lines inside one protocol band.
#[derive(Debug, Clone)]
pub struct SpectrumBandRun {
    pub run: usize,
    pub shown: bool,
    pub unmeasured: bool,
    pub lines: Vec<SpectrumLine>,
}

/// One protocol band: `protocol` is the lower-case protocol name; `boundary` is the band start
/// in percent, `None` for the first band.
#[derive(Debug, Clone)]
pub struct SpectrumBand {
    pub protocol: String,
    pub boundary: Option<f64>,
    pub runs: Vec<SpectrumBandRun>,
}

impl SpectrumBand {
    fn boundary_pct(&self) -> Option<String> {
        self.boundary.map(|b| format!("{b:.4}"))
    }

    /// Whether `live`, a space-separated list of protocol names, names this band.
    fn is_live(&self, live: &str) -> bool {
        live.split(' ').any(|p| p == self.protocol)
    }
}

/// One label: `x` the leader in percent of the plate, `width` the box width in px.
#[derive(Debug, Clone)]
pub struct SpectrumLabel {
    pub code: String,
    pub text: String,
    pub tone: Tone,
    pub row: usize,
    pub x: f64,
    pub width: f64,
}

impl SpectrumLabel {
    fn x_pct(&self) -> String {
        format!("{:.4}", self.x)
    }

    fn width_px(&self) -> String {
        format!("{:.2}", self.width)
    }

    /// The leader's height: the first leader plus one label row per row (8, 22, 36 px on the
    /// desktop; the phone tokens give 6, 18, 30 px).
    fn rise(&self) -> String {
        format!(
            "calc(var(--spectrum-label-leader) + {} * var(--spectrum-label-row))",
            self.row
        )
    }
}

/// One run's placed labels.
#[derive(Debug, Clone)]
pub struct SpectrumLabelSet {
    pub run: usize,
    pub shown: bool,
    pub labels: Vec<SpectrumLabel>,
}

/// A labelled band of the spectrum axis, in percent of its width.
#[derive(Debug, Clone)]
pub struct SpectrumRegion {
    pub label: String,
    pub x: f64,
    pub width: f64,
    /// The section's letter grade, `None` for a section without a score.
    pub grade: Option<String>,
}

impl SpectrumRegion {
    fn x_pct(&self) -> String {
        format!("{:.4}", self.x)
    }

    fn width_pct(&self) -> String {
        format!("{:.4}", self.width)
    }

    /// The name the phone width shows instead of a long one.
    fn short(&self) -> Option<&'static str> {
        (self.label == "EMAIL").then_some("MAIL")
    }
}

/// The replay gutter right of the plate: the run on display. `run` is absent when the run has
/// no place in the stored history.
#[derive(Debug, Clone)]
pub struct SpectrumGutter {
    pub run: Option<usize>,
    pub date: String,
    pub grade: String,
}

/// One run's region grades as (protocol, letter) pairs, the data the replay copies into the
/// region labels.
#[derive(Debug, Clone)]
pub struct SpectrumRunGrades {
    pub run: usize,
    pub grades: Vec<(String, String)>,
}

/// A tick on the spectrum axis at `x` percent of the plate; major at the ends and region bounds.
#[derive(Debug, Clone)]
pub struct SpectrumTick {
    pub x: f64,
    pub major: bool,
    pub last: bool,
}

impl SpectrumTick {
    fn x_pct(&self) -> String {
        format!("{:.4}", self.x)
    }
}

/// The label band's height in px while at most three rows are needed.
const LABEL_BAND_ROWS: usize = 3;

/// The spectrum plate; `rows` is the label band's row count, `line_count` sizes the scroll box.
#[derive(Debug, Clone)]
pub struct SpectrumView {
    pub label: String,
    pub line_count: usize,
    pub rows: usize,
    pub bands: Vec<SpectrumBand>,
    pub label_sets: Vec<SpectrumLabelSet>,
    pub regions: Vec<SpectrumRegion>,
    pub gutter: Option<SpectrumGutter>,
    pub run_grades: Vec<SpectrumRunGrades>,
}

pub(crate) fn line_tone(tone: crate::spectrum::LineTone) -> Tone {
    use crate::spectrum::LineTone;
    match tone {
        LineTone::Pass => Tone::Pass,
        LineTone::Warn => Tone::Warn,
        LineTone::Fail => Tone::Fail,
        LineTone::Fixed => Tone::Fix,
        LineTone::Muted => Tone::Untested,
    }
}

impl SpectrumView {
    /// The axis ticks: one per line boundary, major at the plate's ends and the region bounds.
    fn ticks(&self) -> Vec<SpectrumTick> {
        let count = self.line_count;
        let step = 100.0 / count.max(1) as f64;
        (0..=count)
            .map(|i| {
                let x = i as f64 * step;
                SpectrumTick {
                    x,
                    major: i == 0
                        || i == count
                        || self.regions.iter().any(|r| (r.x - x).abs() < 1e-6),
                    last: i == count,
                }
            })
            .collect()
    }

    /// The label band's height in px: 56 up to three rows, one row more each beyond.
    /// The band: `spectrum.labelBand` for up to three rows, one label row more per extra row.
    fn band_height(&self) -> String {
        let extra = self.rows.saturating_sub(LABEL_BAND_ROWS);
        format!("calc(var(--spectrum-label-band) + {extra} * var(--spectrum-label-row))")
    }

    /// The view with each region labelled by its section's letter grade; `grades` maps a
    /// protocol name to it, a protocol without an entry keeps its name only.
    pub fn with_grades(mut self, grades: &std::collections::BTreeMap<String, String>) -> Self {
        for region in &mut self.regions {
            region.grade = grades.get(&region.label.to_lowercase()).cloned();
        }
        self
    }

    /// The view with the replay gutter naming the run on display.
    pub fn with_gutter(mut self, gutter: SpectrumGutter) -> Self {
        self.gutter = Some(gutter);
        self
    }

    /// The view with every run's region grades, for the replay.
    pub fn with_run_grades(mut self, run_grades: Vec<SpectrumRunGrades>) -> Self {
        self.run_grades = run_grades;
        self
    }

    /// One line's share of the plate width, the width of its hit target.
    fn pitch_pct(&self) -> String {
        format!("{:.4}", 100.0 / self.line_count.max(1) as f64)
    }

    /// The view of a layout: percent positions from px, one band per protocol group, one run
    /// group and one label set per run, `shown` marking the run on display.
    pub fn from_layout(layout: &crate::spectrum::Layout, label: &str, shown: usize) -> Self {
        use crate::spectrum::SectionState;
        let plate = layout.plate_width;
        let bands = layout
            .groups
            .iter()
            .enumerate()
            .map(|(gi, g)| {
                let protocol = layout.lines[g.first].protocol;
                let runs = layout
                    .runs
                    .iter()
                    .enumerate()
                    .map(|(k, run)| SpectrumBandRun {
                        run: k,
                        shown: k == shown,
                        unmeasured: run.sections.get(&protocol) == Some(&SectionState::Unmeasured),
                        lines: (g.first..g.first + g.len)
                            .map(|i| SpectrumLine {
                                code: layout.lines[i].id.to_string(),
                                x: layout.lines[i].x / plate * 100.0,
                                height: run.heights[i] * 100.0,
                                tone: line_tone(run.tones[i]),
                            })
                            .collect(),
                    })
                    .collect();
                SpectrumBand {
                    protocol: protocol.as_str().to_string(),
                    boundary: (gi > 0).then(|| g.start / plate * 100.0),
                    runs,
                }
            })
            .collect();
        let label_sets = layout
            .runs
            .iter()
            .enumerate()
            .map(|(k, run)| SpectrumLabelSet {
                run: k,
                shown: k == shown,
                labels: run
                    .labels
                    .iter()
                    .map(|l| SpectrumLabel {
                        code: layout.lines[l.line].id.to_string(),
                        text: l.text.clone(),
                        tone: line_tone(run.tones[l.line]),
                        row: l.row,
                        x: l.leader / plate * 100.0,
                        width: l.width,
                    })
                    .collect(),
            })
            .collect();
        let regions = layout
            .groups
            .iter()
            .map(|g| SpectrumRegion {
                label: layout.lines[g.first].protocol.as_str().to_uppercase(),
                x: g.start / plate * 100.0,
                width: g.width / plate * 100.0,
                grade: None,
            })
            .collect();
        SpectrumView {
            label: label.to_string(),
            line_count: layout.lines.len(),
            rows: layout.runs.iter().map(|r| r.rows).max().unwrap_or(0),
            bands,
            label_sets,
            regions,
            gutter: None,
            run_grades: Vec::new(),
        }
    }
}

/// One run of the trace; `current` marks the run on display, `no` its place in the stored
/// history when that is known.
#[derive(Debug, Clone)]
pub struct TraceRow {
    pub no: Option<usize>,
    pub date: String,
    pub grade: String,
    pub cells: Vec<Tone>,
    pub current: bool,
}

/// The per-line trace, newest run first; the edge dates, the caption and the hint come with it,
/// `table_href` is the history table's address when the domain has one.
#[derive(Debug, Clone)]
pub struct TraceView {
    pub rows: Vec<TraceRow>,
    pub newest: String,
    pub oldest: String,
    pub caption: String,
    pub hint: String,
    pub table_href: Option<String>,
}

/// One check result.
#[derive(Debug, Clone)]
pub struct CheckRowView {
    pub code: String,
    /// The line code shown in the side column ("HTTP\u{b7}21"); `code` shows when it is `None`.
    pub line: Option<String>,
    pub tone: Tone,
    pub title: String,
    pub since: Option<String>,
    pub body: String,
    pub fix: Option<String>,
    /// The meta line's text, "<Protocol> \u{b7} <source>".
    pub meta: Option<String>,
    pub evidence_href: Option<String>,
    pub guide_href: Option<String>,
}

/// An evidence block; `feeds` lists the checks it backs.
#[derive(Debug, Clone)]
pub struct EvidenceBlockView {
    pub id: String,
    pub title: String,
    pub feeds: Vec<String>,
    pub shape: EvidenceShape,
}

/// One change between two runs.
#[derive(Debug, Clone)]
pub struct DiffEntry {
    pub kind: String,
    pub tone: Tone,
    pub text: String,
    /// The right-hand column: a check's line code, the protocol of an evidence change.
    pub protocol: String,
}

/// The diff against the previous run; `note` explains an empty diff. An empty `title` or
/// `summary` is not rendered.
#[derive(Debug, Clone)]
pub struct DiffBlockView {
    pub title: String,
    pub summary: String,
    pub entries: Vec<DiffEntry>,
    pub note: Option<String>,
}

/// One option of the grouping switch.
#[derive(Debug, Clone)]
pub struct GroupingOption {
    pub value: String,
    pub label: String,
    pub pressed: bool,
}

/// The grouping switch.
#[derive(Debug, Clone)]
pub struct GroupingSwitchView {
    pub label: String,
    pub options: Vec<GroupingOption>,
}

/// The query form.
#[derive(Debug, Clone)]
pub struct QueryInputView {
    pub action: String,
    pub name: String,
    pub label: String,
    pub placeholder: String,
    pub value: Option<String>,
    pub submit: String,
}

/// Defines the private template struct and the public render function of one component.
macro_rules! component {
    ($name:ident, $template:ident, $view:ident, $source:literal) => {
        #[derive(Template)]
        #[template(source = $source, ext = "html")]
        struct $template<'a> {
            view: &'a $view,
        }

        #[doc = concat!("Renders the `", stringify!($name), "` component.")]
        pub fn $name(view: &$view) -> askama::Result<String> {
            $template { view }.render()
        }
    };
}

component!(
    grade,
    GradeTemplate,
    GradeView,
    "{% import \"components/grade.html\" as c %}{% call c::grade(view) %}{% endcall %}"
);
component!(
    run_line,
    RunLineTemplate,
    RunLineView,
    "{% import \"components/run_line.html\" as c %}{% call c::run_line(view) %}{% endcall %}"
);
component!(
    progress_ray,
    ProgressRayTemplate,
    ProgressRayView,
    "{% import \"components/progress_ray.html\" as c %}{% call c::progress_ray(view) %}{% endcall %}"
);
component!(
    spectrum,
    SpectrumTemplate,
    SpectrumView,
    "{% import \"components/spectrum.html\" as c %}{% call c::spectrum(view, \"\") %}{% endcall %}"
);

#[derive(Template)]
#[template(
    source = "{% import \"components/spectrum.html\" as c %}{% call c::spectrum(view, live) %}{% endcall %}",
    ext = "html"
)]
struct SpectrumLiveTemplate<'a> {
    view: &'a SpectrumView,
    live: &'a str,
}

/// Renders the `spectrum` component with the bands named in `live` (space-separated protocol
/// names) marked as measured by the run on display.
pub fn spectrum_live(view: &SpectrumView, live: &str) -> askama::Result<String> {
    SpectrumLiveTemplate { view, live }.render()
}
component!(
    trace,
    TraceTemplate,
    TraceView,
    "{% import \"components/trace.html\" as c %}{% call c::trace(view) %}{% endcall %}"
);
component!(
    check_row,
    CheckRowTemplate,
    CheckRowView,
    "{% import \"components/check_row.html\" as c %}{% call c::check_row(view) %}{% endcall %}"
);
component!(
    evidence_block,
    EvidenceBlockTemplate,
    EvidenceBlockView,
    "{% import \"components/evidence_block.html\" as c %}{% call c::evidence_block(view) %}{% endcall %}"
);
component!(
    diff_block,
    DiffBlockTemplate,
    DiffBlockView,
    "{% import \"components/diff_block.html\" as c %}{% call c::diff_block(view) %}{% endcall %}"
);
component!(
    grouping_switch,
    GroupingSwitchTemplate,
    GroupingSwitchView,
    "{% import \"components/grouping_switch.html\" as c %}{% call c::grouping_switch(view) %}{% endcall %}"
);
component!(
    query_input,
    QueryInputTemplate,
    QueryInputView,
    "{% import \"components/query_input.html\" as c %}{% call c::query_input(view) %}{% endcall %}"
);
