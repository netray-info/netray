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
    pub ttl: u32,
    pub data: String,
}

/// A key and its value.
#[derive(Debug, Clone)]
pub struct Field {
    pub key: String,
    pub value: String,
}

/// One node of a flattened tree; `depth` is its nesting level.
#[derive(Debug, Clone)]
pub struct TreeItem {
    pub depth: u8,
    pub text: String,
}

/// The shape of an evidence block's content.
#[derive(Debug, Clone)]
pub enum EvidenceShape {
    Records(Vec<RecordRow>),
    Table {
        columns: Vec<String>,
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

/// A spectrum line; `x` and `height` are percent of the plate.
#[derive(Debug, Clone)]
pub struct SpectrumLine {
    pub code: String,
    pub x: f32,
    pub height: f32,
    pub tone: Tone,
    pub label: Option<String>,
}

impl SpectrumLine {
    /// The line's top edge, in percent of the plate measured from its top.
    fn top(&self) -> f32 {
        100.0 - self.height
    }

    fn is_fix(&self) -> bool {
        self.tone == Tone::Fix
    }
}

/// A labelled band of the spectrum plate, in percent of its width.
#[derive(Debug, Clone)]
pub struct SpectrumRegion {
    pub label: String,
    pub x: f32,
    pub width: f32,
}

/// The spectrum plate.
#[derive(Debug, Clone)]
pub struct SpectrumView {
    pub label: String,
    pub lines: Vec<SpectrumLine>,
    pub regions: Vec<SpectrumRegion>,
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
