//! The result page: the views it renders from a stored entry or a fresh run, and the router of
//! `/`, `/{domain}`, `/{domain}/history` and `/{domain}/run`.

mod run;
pub mod views;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::Router;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, Request, State};
use axum::http::header::LOCATION;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures::future::BoxFuture;
use netray_common::ip_extract::IpExtractor;
use netray_engine::catalogue::catalogue;
use netray_model::run::Run;
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{Entry, HistoryStore, Recorded};
use percent_encoding::utf8_percent_encode;

use self::views::{Grouping, IssueGroup, Links};
use super::errors::{LOCATION_SET, query_view, refused_page};
use super::headers::secure;
use super::{Frame, page, query_redirect, server_error};
use crate::api::DomainRuns;
use crate::components::{
    self, CheckRowView, DiffBlockView, GradeView, GroupingOption, GroupingSwitchView,
    QueryInputView, RunLineView, SpectrumView,
};
use crate::entries::view::{grade_letter, grade_tone};
use crate::evidence::{self, entry_run_line};
use crate::spectrum::{self, RunInput};

/// The handler a request for a segment that is not a domain goes to.
type Fallback = Arc<dyn Fn(Request) -> BoxFuture<'static, Response> + Send + Sync>;

/// Entries the spectrum and the trace show.
const SHOWN: usize = 24;

const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

/// Single segments that carry a dot and still are no domain.
const ROOT_FILES: [&str; 7] = [
    "robots.txt",
    "sitemap.xml",
    "favicon.svg",
    "favicon.ico",
    "bimi-logo.svg",
    "index.html",
    ".well-known",
];

/// What the result routes need.
#[derive(Clone)]
pub struct ResultState {
    store: Arc<HistoryStore>,
    runs: Arc<dyn DomainRuns>,
    ttl_seconds: u64,
    ip: Arc<IpExtractor>,
    fallback: Fallback,
}

impl ResultState {
    /// `store` the history, `runs` the domain seam, `ttl_seconds` the window a stored entry is fresh.
    pub fn new(store: Arc<HistoryStore>, runs: Arc<dyn DomainRuns>, ttl_seconds: u64) -> Self {
        Self {
            store,
            runs,
            ttl_seconds,
            ip: Arc::new(IpExtractor::new(&[])),
            fallback: Arc::new(|_| Box::pin(super::not_found())),
        }
    }

    /// The client-address extractor.
    pub fn with_ip_extractor(mut self, ip: Arc<IpExtractor>) -> Self {
        self.ip = ip;
        self
    }

    /// The handler a request for a single segment that is not a domain falls to.
    pub fn with_fallback(mut self, fallback: Fallback) -> Self {
        self.fallback = fallback;
        self
    }

    /// The client-address extractor.
    pub fn ip_extractor(&self) -> &IpExtractor {
        &self.ip
    }
}

/// `GET /`, `GET /{domain}`, `GET /{domain}/history` and `POST /{domain}/run`.
pub fn router(state: ResultState) -> Router {
    Router::new()
        .route("/", get(idle))
        .route("/{domain}", get(show).fallback(wrong_method))
        .route("/{domain}/history", get(history).fallback(wrong_method))
        .route("/{domain}/run", post(run::run).fallback(wrong_method))
        .with_state(state)
}

fn domain_segment(path: Result<Path<String>, PathRejection>) -> Option<String> {
    let Path(segment) = path.ok()?;
    (segment.contains('.') && !ROOT_FILES.contains(&segment.as_str())).then_some(segment)
}

/// An unsupported method: a non-domain segment goes to the fallback, a domain answers 405.
async fn wrong_method(
    State(state): State<ResultState>,
    path: Result<Path<String>, PathRejection>,
    request: Request,
) -> Response {
    if domain_segment(path).is_none() {
        return (state.fallback)(request).await;
    }
    StatusCode::METHOD_NOT_ALLOWED.into_response()
}

fn href_of(domain: &str) -> String {
    utf8_percent_encode(domain, LOCATION_SET).to_string()
}

/// The normalised domain of `segment`, or the response that answers the request instead: the
/// refusal page, or for a GET a redirect to `/<normalised><tail>`.
fn resolve(
    state: &ResultState,
    segment: &str,
    tail: &str,
    is_get: bool,
) -> Result<String, Box<Response>> {
    let domain = state
        .runs
        .validate(segment)
        .map_err(|refusal| Box::new(refused_page(segment, &refusal.message)))?;
    if domain == segment {
        return Ok(domain);
    }
    if is_get {
        let location = format!("/{}{tail}", href_of(&domain));
        return Err(Box::new(secure(
            (StatusCode::MOVED_PERMANENTLY, [(LOCATION, location)]).into_response(),
        )));
    }
    Err(Box::new(refused_page(
        segment,
        "write the domain in lower case without a trailing dot",
    )))
}

async fn idle(uri: Uri) -> Response {
    if uri
        .query()
        .is_some_and(|q| q.split('&').any(|p| p.split('=').next() == Some("d")))
    {
        return query_redirect(uri).await;
    }
    page(
        StatusCode::OK,
        "no-cache",
        &IdlePage {
            frame: Frame::new(
                "netray - check a domain",
                "Every check of a domain, in one spectrum.",
                "/",
            ),
            query: query_view(None),
        },
    )
}

async fn history(
    State(state): State<ResultState>,
    path: Result<Path<String>, PathRejection>,
    request: Request,
) -> Response {
    let Some(segment) = domain_segment(path) else {
        return (state.fallback)(request).await;
    };
    let domain = match resolve(&state, &segment, "/history", true) {
        Ok(domain) => domain,
        Err(response) => return *response,
    };
    let entries = match state.store.entries(&domain).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(domain = %domain, error = %e, "history lookup failed");
            return server_error();
        }
    };
    let status = if entries.is_empty() {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::OK
    };
    let cache = if entries.is_empty() {
        "no-store"
    } else {
        "no-cache"
    };
    page(
        status,
        cache,
        &HistoryPage {
            frame: Frame::new(
                &format!("{domain} history - netray"),
                "Every stored result of one domain.",
                &format!("/{}/history", href_of(&domain)),
            ),
            href: href_of(&domain),
            rows: entries.iter().map(history_row).collect(),
            domain,
        },
    )
}

fn history_row(entry: &Entry) -> HistoryRow {
    HistoryRow {
        id: entry.id.clone(),
        created: views::utc_minute(entry.created_at),
        last_seen: views::utc_minute(entry.last_seen_at),
        grade: grade_letter(entry.grade).to_string(),
        profile: entry.profile_version.to_string(),
        changes: entry
            .diff
            .as_ref()
            .map_or_else(|| "first".to_string(), |d| d.checks.len().to_string()),
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

async fn show(
    State(state): State<ResultState>,
    path: Result<Path<String>, PathRejection>,
    request: Request,
) -> Response {
    let Some(segment) = domain_segment(path) else {
        return (state.fallback)(request).await;
    };
    let domain = match resolve(&state, &segment, "", true) {
        Ok(domain) => domain,
        Err(response) => return *response,
    };
    let entries = match state.store.entries(&domain).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(domain = %domain, error = %e, "history lookup failed");
            return server_error();
        }
    };
    full_page(&state, &domain, &entries, StatusCode::OK, "no-cache", None)
}

/// The whole result page of `domain` from its history `entries` (newest first); `refusal` puts a
/// refused run's line in the header and keeps the page from starting a run on its own.
fn full_page(
    state: &ResultState,
    domain: &str,
    entries: &[Entry],
    status: StatusCode,
    cache_control: &'static str,
    refusal: Option<RunLineBody>,
) -> Response {
    let state_word = state_word(entries, state.ttl_seconds);
    let autostart = refusal.is_none() && state_word != "fresh";
    let Some(parts) = stored_parts(domain, entries, autostart, None, refusal) else {
        return server_error();
    };
    page(
        status,
        cache_control,
        &ResultPage {
            frame: Frame::new(
                &format!("{domain} - netray"),
                "Every check of one domain, in one spectrum.",
                &format!("/{}", href_of(domain)),
            ),
            state: state_word,
            domain: domain.to_string(),
            parts,
        },
    )
}

fn is_fresh(entries: &[Entry], ttl_seconds: u64) -> bool {
    entries.first().is_some_and(|e| {
        now().saturating_sub(e.last_seen_at) < i64::try_from(ttl_seconds).unwrap_or(i64::MAX)
    })
}

fn state_word(entries: &[Entry], ttl_seconds: u64) -> &'static str {
    match (entries.is_empty(), is_fresh(entries, ttl_seconds)) {
        (true, _) => "none",
        (false, true) => "fresh",
        (false, false) => "stale",
    }
}

/// What the run line says.
enum RunLineBody {
    Line(RunLineView),
    Text(String),
    /// The lead text of a refusal and the seconds its countdown starts at.
    Refusal {
        lead: String,
        retry: u64,
    },
}

/// What one protocol's evidence element holds.
enum EvidenceInner {
    Empty,
    Link { href: String, outcome: String },
    Section(String),
}

/// Everything the page shows apart from the frame, before it is rendered into parts.
struct Shown {
    grade: Option<GradeView>,
    run_line: RunLineBody,
    share: Option<String>,
    plate: String,
    trace: String,
    sentences: Vec<(String, String)>,
    refreshing: bool,
    diff: Option<DiffBlockView>,
    groups: Vec<IssueGroup>,
    passed: Vec<CheckRowView>,
    evidence: Vec<EvidenceInner>,
    recheck: bool,
}

impl Shown {
    fn none() -> Self {
        Shown {
            grade: None,
            run_line: RunLineBody::Text("not checked yet".to_string()),
            share: None,
            plate: String::new(),
            trace: String::new(),
            sentences: Vec::new(),
            refreshing: false,
            diff: None,
            groups: Vec::new(),
            passed: Vec::new(),
            evidence: PROTOCOLS.iter().map(|_| EvidenceInner::Empty).collect(),
            recheck: false,
        }
    }
}

/// The rendered page parts, one per fragment id; the evidence elements run dns to ip.
struct Parts {
    header: String,
    grade: String,
    run_line: String,
    progress: String,
    share: String,
    spectrum: String,
    diff: String,
    issues: String,
    evidence: Vec<String>,
    passed: String,
}

impl Parts {
    /// The end fragments in the order the stream sends them.
    fn fragments(&self) -> Vec<(String, &str)> {
        let mut out = vec![
            ("result-grade".to_string(), self.grade.as_str()),
            ("run-line".to_string(), self.run_line.as_str()),
            ("progress".to_string(), self.progress.as_str()),
            ("result-share".to_string(), self.share.as_str()),
            ("spectrum".to_string(), self.spectrum.as_str()),
            ("diff".to_string(), self.diff.as_str()),
            ("issues".to_string(), self.issues.as_str()),
        ];
        for (p, html) in PROTOCOLS.iter().zip(&self.evidence) {
            out.push((format!("evidence-{}", p.as_str()), html.as_str()));
        }
        out.push(("passed".to_string(), self.passed.as_str()));
        out
    }
}

fn warn_render<T>(what: &str, result: Result<T, askama::Error>) -> Option<T> {
    result
        .inspect_err(|e| tracing::warn!(part = what, error = %e, "result part render failed"))
        .ok()
}

/// The parts of the history `entries` (newest first) of `domain`; `recorded` is the store's
/// answer to a run that just finished, `run_line` replaces the run line.
fn stored_parts(
    domain: &str,
    entries: &[Entry],
    autostart: bool,
    recorded: Option<&Recorded>,
    run_line: Option<RunLineBody>,
) -> Option<Parts> {
    let mut shown = if entries.is_empty() {
        Shown::none()
    } else {
        stored_shown(entries, recorded)?
    };
    if let Some(run_line) = run_line {
        shown.run_line = run_line;
    }
    assemble(domain, autostart, shown)
}

fn grade_view(grade: Grade) -> GradeView {
    GradeView {
        letter: grade_letter(grade).to_string(),
        score: None,
        tone: grade_tone(grade),
    }
}

fn rows_of(entries: &[Entry]) -> Vec<(i64, Grade)> {
    entries
        .iter()
        .take(SHOWN)
        .map(|e| (e.created_at, e.grade))
        .collect()
}

/// The spectrum plate and the trace of `current` over `older`; `live` names the bands the run
/// on display measured.
fn figure_parts(
    domain: &str,
    current: &RunInput<'static>,
    older: &[RunInput<'static>],
    rows: &[(i64, Grade)],
    live: &str,
) -> Option<(String, String)> {
    let cat = catalogue();
    let width = (cat.entries().len() as f64 * spectrum::line_pitch_min()) as u32;
    let layout = match spectrum::layout(current, older, cat, width) {
        Ok(layout) => layout,
        Err(e) => {
            tracing::warn!(error = ?e, "spectrum layout failed");
            return None;
        }
    };
    let label = format!("Check results of {domain}");
    let plate = warn_render(
        "spectrum",
        components::spectrum_live(&SpectrumView::from_layout(&layout, &label, 0), live),
    )?;
    let trace = warn_render(
        "trace",
        components::trace(&views::trace_view(&layout, rows)),
    )?;
    Some((plate, trace))
}

/// What the latest of `entries` (newest first, not empty) shows.
fn stored_shown(entries: &[Entry], recorded: Option<&Recorded>) -> Option<Shown> {
    let entry = entries.first()?;
    let cat = catalogue();
    let inputs: Vec<RunInput<'static>> = entries
        .iter()
        .take(SHOWN)
        .map(|e| views::layout_input_or_current(&e.run))
        .collect();
    let (plate, trace) = figure_parts(
        &entry.domain,
        &inputs[0],
        &inputs[1..],
        &rows_of(entries),
        "",
    )?;
    let chosen: Vec<&Entry> = entries.iter().take(SHOWN).collect();
    let mut since: BTreeMap<CheckId, String> = BTreeMap::new();
    for check in &entry.run.checks {
        let points: Vec<(i64, Option<Status>)> = chosen
            .iter()
            .rev()
            .map(|e| {
                let c = e.run.checks.iter().find(|c| c.id == check.id);
                (e.created_at, c.map(|c| c.status))
            })
            .collect();
        if let Some(start) = views::issue_since_present(&points) {
            since.insert(check.id.clone(), start);
        }
    }
    let diff = match recorded {
        Some(recorded) => views::recorded_diff_view(recorded, Some(entry), cat).block,
        None => views::diff_view(entry.diff.as_ref(), cat).block,
    };
    Some(Shown {
        grade: Some(grade_view(entry.grade)),
        run_line: RunLineBody::Line(entry_run_line(entry.created_at, &entry.id)),
        share: entry.run.complete.then(|| entry.id.clone()),
        plate,
        trace,
        sentences: sentences_of(chosen.iter().rev().copied()),
        refreshing: false,
        diff: Some(diff),
        groups: views::issues(
            &entry.run,
            cat,
            Links::Entry(&entry.id),
            Grouping::Owner,
            &since,
        ),
        passed: views::passed_checks(&entry.run, cat),
        evidence: PROTOCOLS
            .iter()
            .map(|p| EvidenceInner::Link {
                href: format!("/r/{}/{}", entry.id, p.as_str()),
                outcome: views::section_outcome(&entry.run, *p),
            })
            .collect(),
        recheck: true,
    })
}

/// The spectrum plate and trace of the finished run `done` over `entries`, its own row graded
/// `grade`.
fn done_figure(
    domain: &str,
    done: &Run,
    grade: Grade,
    entries: &[Entry],
) -> Option<(String, String)> {
    let current = views::layout_input_or_current(done);
    let older: Vec<RunInput<'static>> = entries
        .iter()
        .take(SHOWN - 1)
        .map(|e| views::layout_input_or_current(&e.run))
        .collect();
    let mut rows = vec![(now(), grade)];
    rows.extend(rows_of(entries).into_iter().take(SHOWN - 1));
    figure_parts(domain, &current, &older, &rows, "")
}

/// One evidence section per protocol over `done`.
fn done_evidence(done: &Run, run_line: impl Fn() -> RunLineView) -> Option<Vec<EvidenceInner>> {
    PROTOCOLS
        .iter()
        .map(|p| {
            warn_render(
                "evidence",
                evidence::render_section(&evidence::section(done, *p, run_line())),
            )
            .map(EvidenceInner::Section)
        })
        .collect()
}

/// What a finished run of `domain` that was not stored shows: `done` the run, `entries` the
/// history before it, `recorded` the store's `NotStored` answer.
fn not_stored_shown(
    domain: &str,
    done: &Run,
    recorded: &Recorded,
    entries: &[Entry],
) -> Option<Shown> {
    let cat = catalogue();
    let grade = if done.grade == Grade::F {
        Grade::F
    } else {
        Grade::Incomplete
    };
    let (plate, trace) = done_figure(domain, done, grade, entries)?;
    let evidence = done_evidence(done, evidence::incomplete_run_line)?;
    Some(Shown {
        grade: Some(grade_view(grade)),
        run_line: RunLineBody::Line(evidence::incomplete_run_line()),
        share: None,
        plate,
        trace,
        sentences: sentences_through(entries, done),
        refreshing: false,
        diff: Some(views::recorded_diff_view(recorded, entries.first(), cat).block),
        groups: views::issues(done, cat, Links::Live, Grouping::Owner, &BTreeMap::new()),
        passed: views::passed_checks(done, cat),
        evidence,
        recheck: true,
    })
}

/// What a complete run of `domain` whose record failed shows: its own grade, "not recorded", no
/// share link.
fn unrecorded_shown(domain: &str, done: &Run, entries: &[Entry]) -> Option<Shown> {
    let cat = catalogue();
    let (plate, trace) = done_figure(domain, done, done.grade, entries)?;
    let evidence = done_evidence(done, evidence::live_run_line)?;
    Some(Shown {
        grade: Some(grade_view(done.grade)),
        run_line: RunLineBody::Line(RunLineView {
            parts: vec![views::NOT_RECORDED.to_string()],
        }),
        share: None,
        plate,
        trace,
        sentences: sentences_through(entries, done),
        refreshing: false,
        diff: Some(views::not_recorded_view().block),
        groups: views::issues(done, cat, Links::Live, Grouping::Owner, &BTreeMap::new()),
        passed: views::passed_checks(done, cat),
        evidence,
        recheck: true,
    })
}

/// The history sentence of every catalogue line over `entries`, oldest first.
fn sentences_of<'a>(entries: impl Iterator<Item = &'a Entry>) -> Vec<(String, String)> {
    let points: Vec<(i64, &Run)> = entries.map(|e| (e.created_at, &e.run)).collect();
    views::history_sentences(&points)
}

/// The sentences over the history `entries` (newest first) with the run `done`, not stored, as
/// the newest point dated now.
fn sentences_through(entries: &[Entry], done: &Run) -> Vec<(String, String)> {
    let mut points: Vec<(i64, &Run)> = entries
        .iter()
        .take(SHOWN - 1)
        .rev()
        .map(|e| (e.created_at, &e.run))
        .collect();
    points.push((now(), done));
    views::history_sentences(&points)
}

/// The entry a stored `recorded` answer names, built from the finished run `done` when the
/// history read does not return it.
fn recorded_entry(domain: &str, done: &Run, recorded: &Recorded) -> Option<Entry> {
    let (id, diff) = match recorded {
        Recorded::First { id } | Recorded::Identical { id } => (id, None),
        Recorded::Changed { id, diff } => (id, Some(diff.clone())),
        Recorded::NotStored { .. } => return None,
    };
    let at = now();
    Some(Entry {
        id: id.clone(),
        domain: domain.to_string(),
        created_at: at,
        last_seen_at: at,
        predecessor_id: None,
        profile_version: done.profile_version,
        grade: done.grade,
        fingerprint: String::new(),
        run: done.clone(),
        diff,
        engine_version: done.engine_version.clone(),
    })
}

fn grouping_switch() -> GroupingSwitchView {
    let option = |value: &str, pressed| GroupingOption {
        value: value.to_string(),
        label: value.to_string(),
        pressed,
    };
    GroupingSwitchView {
        label: "Group issues by".to_string(),
        options: vec![
            option("owner", true),
            option("severity", false),
            option("protocol", false),
        ],
    }
}

#[derive(Template)]
#[template(path = "partials/result_run_line.html")]
struct RunLinePart<'a> {
    body: &'a RunLineBody,
}

#[derive(Template)]
#[template(path = "partials/result_progress.html")]
struct ProgressPart<'a> {
    ray: &'a str,
}

#[derive(Template)]
#[template(path = "partials/result_grade.html")]
struct GradePart<'a> {
    grade: &'a Option<GradeView>,
}

#[derive(Template)]
#[template(path = "partials/result_share.html")]
struct SharePart<'a> {
    share: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "partials/result_header.html")]
struct HeaderPart<'a> {
    grade: &'a str,
    domain: &'a str,
    href: &'a str,
    run_line: &'a str,
    progress: &'a str,
    autostart: bool,
    trigger: &'static str,
    trigger_label: &'static str,
    share: &'a str,
}

#[derive(Template)]
#[template(path = "partials/result_spectrum.html")]
struct SpectrumPart<'a> {
    plate: &'a str,
    trace: &'a str,
    sentences: &'a [(String, String)],
    refreshing: bool,
    stepper: bool,
}

#[derive(Template)]
#[template(path = "partials/result_diff.html")]
struct DiffPart<'a> {
    diff: &'a Option<DiffBlockView>,
}

#[derive(Template)]
#[template(path = "partials/result_issues.html")]
struct IssuesPart<'a> {
    switch: &'a GroupingSwitchView,
    groups: &'a [IssueGroup],
}

#[derive(Template)]
#[template(path = "partials/result_evidence.html")]
struct EvidencePart<'a> {
    protocol: &'static str,
    inner: &'a EvidenceInner,
}

#[derive(Template)]
#[template(path = "partials/result_passed.html")]
struct PassedPart<'a> {
    rows: &'a [CheckRowView],
}

fn trimmed<T: Template>(template: &T) -> Result<String, askama::Error> {
    template.render().map(|html| html.trim().to_string())
}

fn run_line_part(body: &RunLineBody) -> Option<String> {
    warn_render("run-line", trimmed(&RunLinePart { body }))
}

/// The progress element around `ray`, the rendered progress ray or nothing.
fn progress_part(ray: &str) -> Option<String> {
    warn_render("progress", trimmed(&ProgressPart { ray }))
}

fn spectrum_part(
    plate: &str,
    trace: &str,
    sentences: &[(String, String)],
    refreshing: bool,
) -> Option<String> {
    warn_render(
        "spectrum",
        trimmed(&SpectrumPart {
            plate,
            trace,
            sentences,
            refreshing,
            stepper: trace.matches("data-run=").count() >= 2,
        }),
    )
}

fn issues_part(groups: &[IssueGroup]) -> Option<String> {
    let switch = grouping_switch();
    warn_render(
        "issues",
        trimmed(&IssuesPart {
            switch: &switch,
            groups,
        }),
    )
}

fn evidence_part(protocol: Protocol, inner: &EvidenceInner) -> Option<String> {
    warn_render(
        "evidence",
        trimmed(&EvidencePart {
            protocol: protocol.as_str(),
            inner,
        }),
    )
}

fn assemble(domain: &str, autostart: bool, shown: Shown) -> Option<Parts> {
    let href = href_of(domain);
    let (trigger, trigger_label) = if shown.recheck {
        ("recheck", "re-check now")
    } else {
        ("visit", "check now")
    };
    let run_line = run_line_part(&shown.run_line)?;
    let progress = progress_part("")?;
    let grade = warn_render(
        "result-grade",
        trimmed(&GradePart {
            grade: &shown.grade,
        }),
    )?;
    let share = warn_render(
        "result-share",
        trimmed(&SharePart {
            share: &shown.share,
        }),
    )?;
    let header = warn_render(
        "result-header",
        trimmed(&HeaderPart {
            grade: &grade,
            domain,
            href: &href,
            run_line: &run_line,
            progress: &progress,
            autostart,
            trigger,
            trigger_label,
            share: &share,
        }),
    )?;
    let diff = warn_render("diff", trimmed(&DiffPart { diff: &shown.diff }))?;
    let passed = warn_render(
        "passed",
        trimmed(&PassedPart {
            rows: &shown.passed,
        }),
    )?;
    let evidence = PROTOCOLS
        .iter()
        .zip(&shown.evidence)
        .map(|(p, inner)| evidence_part(*p, inner))
        .collect::<Option<Vec<_>>>()?;
    Some(Parts {
        header,
        grade,
        run_line,
        progress,
        share,
        spectrum: spectrum_part(
            &shown.plate,
            &shown.trace,
            &shown.sentences,
            shown.refreshing,
        )?,
        diff,
        issues: issues_part(&shown.groups)?,
        evidence,
        passed,
    })
}

struct HistoryRow {
    id: String,
    created: String,
    last_seen: String,
    grade: String,
    profile: String,
    changes: String,
}

#[derive(Template)]
#[template(path = "pages/result.html")]
struct ResultPage {
    frame: Frame,
    domain: String,
    state: &'static str,
    parts: Parts,
}

#[derive(Template)]
#[template(path = "pages/history.html")]
struct HistoryPage {
    frame: Frame,
    domain: String,
    href: String,
    rows: Vec<HistoryRow>,
}

#[derive(Template)]
#[template(path = "pages/idle.html")]
struct IdlePage {
    frame: Frame,
    query: QueryInputView,
}
