//! The ten R44 components of the V2 design system. Each is an askama macro under
//! `templates/components/` and a stylesheet under `static/`; the functions here render a macro
//! from a view. Styling lives in CSS keyed on classes and `data-*` attributes, never in markup.

use askama::Template;

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

/// A DNS record row of an evidence block.
#[derive(Debug, Clone)]
pub struct RecordRow {
    pub owner: String,
    pub rtype: String,
    pub ttl: String,
    pub data: String,
}

/// A key and its value.
#[derive(Debug, Clone)]
pub struct Field {
    pub key: String,
    pub value: String,
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
        rows: Vec<Vec<String>>,
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
}

impl SpectrumRegion {
    fn x_pct(&self) -> String {
        format!("{:.4}", self.x)
    }
}

/// The spectrum plate; `rows` is the label band's row count, `line_count` sizes the scroll box.
#[derive(Debug, Clone)]
pub struct SpectrumView {
    pub label: String,
    pub line_count: usize,
    pub rows: usize,
    pub bands: Vec<SpectrumBand>,
    pub label_sets: Vec<SpectrumLabelSet>,
    pub regions: Vec<SpectrumRegion>,
}

fn line_tone(tone: crate::spectrum::LineTone) -> Tone {
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
                label: layout.lines[g.first].protocol.as_str().to_string(),
                x: g.start / plate * 100.0,
                width: g.width / plate * 100.0,
            })
            .collect();
        SpectrumView {
            label: label.to_string(),
            line_count: layout.lines.len(),
            rows: layout.runs.iter().map(|r| r.rows).max().unwrap_or(0),
            bands,
            label_sets,
            regions,
        }
    }
}

/// One run of the trace; `current` marks the run on display.
#[derive(Debug, Clone)]
pub struct TraceRow {
    pub date: String,
    pub grade: String,
    pub cells: Vec<Tone>,
    pub current: bool,
}

/// The trace waterfall, newest run first.
#[derive(Debug, Clone)]
pub struct TraceView {
    pub rows: Vec<TraceRow>,
}

/// One check result.
#[derive(Debug, Clone)]
pub struct CheckRowView {
    pub code: String,
    pub tone: Tone,
    pub title: String,
    pub since: Option<String>,
    pub body: String,
    pub fix: Option<String>,
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
    pub protocol: String,
}

/// The diff against the previous run; `note` explains an empty diff.
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
    "{% import \"components/spectrum.html\" as c %}{% call c::spectrum(view) %}{% endcall %}"
);
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
