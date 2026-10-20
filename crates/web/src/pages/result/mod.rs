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
use axum::extract::{Path, Query, Request, State};
use axum::http::header::LOCATION;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures::future::BoxFuture;
use netray_common::ip_extract::IpExtractor;
use netray_engine::catalogue::catalogue;
use netray_engine::history::score_run;
use netray_engine::profile::Profiles;
use netray_engine::scoring::Score;
use netray_model::run::Run;
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{Entry, HistoryStore, Recorded};
use percent_encoding::utf8_percent_encode;

use self::views::{Grouping, HardFailView, IssueGroup, Links};
use super::errors::{LOCATION_SET, query_view, refused_page, selectors_refused_page};
use super::headers::secure;
use super::{Frame, PageHelp, export_slot, page, query_redirect, server_error};
use crate::api::DomainRuns;
use crate::components::{
    self, CheckRowView, DiffBlockView, EvidenceShape, GradeView, GroupingOption,
    GroupingSwitchView, QueryInputView, RunLineView, SpectrumGutter, SpectrumRunGrades,
    SpectrumView, Tone,
};
use crate::entries::view::{grade_letter, grade_tone};
use crate::evidence;
use crate::export::{self, ExportInput};
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
        .route("/", get(idle).fallback(root_wrong_method))
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
    secure(StatusCode::METHOD_NOT_ALLOWED.into_response())
}

/// An unsupported method on `/`: a 405 with the pages' headers.
async fn root_wrong_method() -> Response {
    secure(StatusCode::METHOD_NOT_ALLOWED.into_response())
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
        &format!("write the domain in its normal form: {domain}"),
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
            legend: crate::grade_legend::legend(),
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
                &format!("{} history - netray", crate::idn::title(&domain)),
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
    // The canonical redirect keeps the query: it carries `dkim_selectors`.
    let tail = request
        .uri()
        .query()
        .map_or_else(String::new, |q| format!("?{q}"));
    let domain = match resolve(&state, &segment, &tail, true) {
        Ok(domain) => domain,
        Err(response) => return *response,
    };
    let selectors = match selectors_of(request.uri()) {
        Ok(selectors) => selectors,
        Err(reason) => return selectors_refused_page(&domain, &reason),
    };
    let entries = match state.store.entries(&domain).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(domain = %domain, error = %e, "history lookup failed");
            return server_error();
        }
    };
    // A page asking for custom DKIM selectors runs them: the stored head answers another
    // question, and the page is the viewer's own.
    let cache = if selectors.is_some() {
        "no-store"
    } else {
        "no-cache"
    };
    full_page(
        &state,
        &domain,
        &entries,
        StatusCode::OK,
        cache,
        None,
        selectors.as_deref(),
    )
}

/// The `dkim_selectors` query parameter of a result page, normalised (tokens joined by commas);
/// `None` when absent or empty (spec v2-dkim-legacy-urls requirement 6).
fn selectors_of(uri: &Uri) -> Result<Option<String>, String> {
    #[derive(serde::Deserialize)]
    struct SelectorQuery {
        dkim_selectors: Option<String>,
    }
    let Query(SelectorQuery { dkim_selectors }) =
        Query::<SelectorQuery>::try_from_uri(uri).map_err(|e| e.body_text())?;
    let list = crate::selectors::parse_dkim_selectors(dkim_selectors.as_deref())?;
    Ok((!list.is_empty()).then(|| list.join(",")))
}

/// The whole result page of `domain` from its history `entries` (newest first); `refusal` puts a
/// refused run's line in the header and keeps the page from starting a run on its own.
/// `selectors` (normalised) goes into the recheck form and starts a run even on a fresh head.
fn full_page(
    state: &ResultState,
    domain: &str,
    entries: &[Entry],
    status: StatusCode,
    cache_control: &'static str,
    refusal: Option<RunLineBody>,
    selectors: Option<&str>,
) -> Response {
    let state_word = state_word(entries, state.ttl_seconds);
    let autostart = refusal.is_none() && (state_word != "fresh" || selectors.is_some());
    let cached = refusal.is_none() && state_word == "fresh" && selectors.is_none();
    let how = StoredHow {
        recorded: None,
        cached,
        ordinal_known: true,
    };
    let Some(parts) = stored_parts(domain, entries, autostart, how, refusal, selectors) else {
        return server_error();
    };
    result_page(status, cache_control, domain, state_word, parts)
}

/// The export slot a run's export fragment fills.
const EXPORT_PLACEHOLDER: &str =
    r#"<div class="inspector-export" id="inspect-export" hidden></div>"#;

/// The export slot of a stored entry: its grade, the score the page shows and every check.
fn entry_export(entry: &Entry) -> Option<String> {
    let shown = entry_score(entry).and_then(|score| shown_score(entry, &score));
    let checks = export::checks_of(&entry.run);
    let errors = export::section_errors(&entry.run, &PROTOCOLS);
    let input = ExportInput {
        target: &entry.domain,
        title: &[],
        band: Some((&PROTOCOLS, &checks)),
        blocks: &entry.run.blocks,
        errors: &errors,
    };
    export_slot(
        format!("netray-{}-{}", export::slug(&entry.domain), entry.id),
        Some((
            export::entry_markdown(&entry.domain, entry.grade, shown, &checks),
            export::csv(&input),
            crate::api::entry_json(entry),
        )),
    )
}

/// The result page around rendered `parts`.
fn result_page(
    status: StatusCode,
    cache_control: &'static str,
    domain: &str,
    state_word: &'static str,
    parts: Parts,
) -> Response {
    page(
        status,
        cache_control,
        &ResultPage {
            frame: Frame::new(
                &format!("{} - netray", crate::idn::title(domain)),
                "Every check of one domain, in one spectrum.",
                &format!("/{}", href_of(domain)),
            ),
            state: state_word,
            domain: domain.to_string(),
            parts,
            help: PageHelp::RESULT,
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
    /// The title of a stored run.
    Title(RunLineView),
    Text(String),
    /// The lead text of a refusal and the seconds its countdown starts at.
    Refusal {
        lead: String,
        retry: u64,
    },
}

/// What one protocol's tab panel holds; `shown` marks the panel of the current tab.
struct EvidenceInner {
    shown: bool,
    panel: Option<evidence::PanelView>,
}

/// The panels before any run: empty, the DNS tab current.
fn empty_panels() -> Vec<EvidenceInner> {
    PROTOCOLS
        .iter()
        .map(|p| EvidenceInner {
            shown: *p == Protocol::Dns,
            panel: None,
        })
        .collect()
}

/// The current tab of `run`: TLS when TLS has a line that is not passing, else the first
/// protocol with one, else DNS.
fn current_protocol(run: &Run) -> Protocol {
    let open = |p: Protocol| {
        run.checks
            .iter()
            .any(|c| c.id.protocol() == p && matches!(c.status, Status::Warn | Status::Fail))
    };
    if open(Protocol::Tls) {
        return Protocol::Tls;
    }
    PROTOCOLS
        .iter()
        .copied()
        .find(|p| open(*p))
        .unwrap_or(Protocol::Dns)
}

/// One panel per protocol over `run`; `entry_id` is the stored entry the "All evidence" links
/// name.
fn panels(run: &Run, domain: &str, entry_id: Option<&str>) -> Vec<EvidenceInner> {
    let current = current_protocol(run);
    PROTOCOLS
        .iter()
        .map(|p| EvidenceInner {
            shown: *p == current,
            panel: Some(evidence::panel(run, *p, domain, entry_id)),
        })
        .collect()
}

fn tab_label(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Dns => "DNS",
        Protocol::Tls => "TLS",
        Protocol::Http => "HTTP",
        Protocol::Email => "Email",
        Protocol::Ip => "IP",
    }
}

/// One tab link of the evidence card.
struct TabView {
    protocol: &'static str,
    label: &'static str,
    current: bool,
}

/// Everything the page shows apart from the frame, before it is rendered into parts.
struct Shown {
    grade: Option<GradeView>,
    date: Option<String>,
    run_line: RunLineBody,
    share: Option<String>,
    plate: String,
    trace: String,
    legend: Legend,
    sentences: Vec<(String, String)>,
    refreshing: bool,
    first_run: Option<FirstRun>,
    diff_heading: Option<String>,
    diff_meta: Option<String>,
    diff: Option<DiffBlockView>,
    hard_fail: Option<HardFailView>,
    groups: Vec<IssueGroup>,
    clean: usize,
    passed: Vec<CheckRowView>,
    evidence: Vec<EvidenceInner>,
    recorded: bool,
    recheck: bool,
}

impl Shown {
    fn none() -> Self {
        Shown {
            grade: None,
            date: None,
            run_line: RunLineBody::Text("not checked yet".to_string()),
            share: None,
            plate: String::new(),
            trace: String::new(),
            legend: Legend::default(),
            sentences: Vec::new(),
            refreshing: false,
            first_run: None,
            diff_heading: None,
            diff_meta: None,
            diff: None,
            hard_fail: None,
            groups: Vec::new(),
            clean: 0,
            passed: Vec::new(),
            evidence: empty_panels(),
            recorded: false,
            recheck: false,
        }
    }
}

/// The rendered page parts, one per fragment id; the evidence elements run dns to ip.
struct Parts {
    header: String,
    hard_fail: String,
    grade: String,
    run_line: String,
    progress: String,
    share: String,
    button: String,
    recheck: bool,
    spectrum: String,
    diff: String,
    issues: String,
    meta: String,
    nav: String,
    evidence: Vec<String>,
    /// The email panel without the selector form, for the stream: its panels replace the page's.
    email_fragment: String,
    passed: String,
    /// The stored entry's export slot, else the empty placeholder.
    export: String,
}

impl Parts {
    /// The end fragments in the order the stream sends them; the check button last, once it
    /// re-checks.
    fn fragments(&self) -> Vec<(String, &str)> {
        let mut out = vec![
            ("result-grade".to_string(), self.grade.as_str()),
            ("run-line".to_string(), self.run_line.as_str()),
            ("progress".to_string(), self.progress.as_str()),
            ("result-share".to_string(), self.share.as_str()),
            ("hard-fail-slot".to_string(), self.hard_fail.as_str()),
            ("spectrum".to_string(), self.spectrum.as_str()),
            ("diff".to_string(), self.diff.as_str()),
            ("issues".to_string(), self.issues.as_str()),
        ];
        for (p, html) in PROTOCOLS.iter().zip(&self.evidence) {
            let html = if *p == Protocol::Email {
                &self.email_fragment
            } else {
                html
            };
            out.push((format!("evidence-{}", p.as_str()), html.as_str()));
        }
        out.push(("evidence-nav".to_string(), self.nav.as_str()));
        out.push(("evidence-meta".to_string(), self.meta.as_str()));
        out.push(("passed".to_string(), self.passed.as_str()));
        out.push(("inspect-export".to_string(), self.export.as_str()));
        if self.recheck {
            out.push(("recheck-button".to_string(), self.button.as_str()));
        }
        out
    }
}

fn warn_render<T>(what: &str, result: Result<T, askama::Error>) -> Option<T> {
    result
        .inspect_err(|e| tracing::warn!(part = what, error = %e, "result part render failed"))
        .ok()
}

/// The parts of the history `entries` (newest first) of `domain`; `recorded` is the store's
/// answer to a run that just finished, `cached` marks the title as answered from a stored
/// entry, `run_line` replaces the run line.
/// `ordinal_known` is false when the head of `entries` stands in for a record the history read
/// did not return: its place in the history is then unknown.
/// How the stored entries reached the page: `recorded` the store's answer to a run that just
/// finished, `cached` when a fresh entry answered without a run, `ordinal_known` when the newest
/// entry's place in the history is known.
struct StoredHow<'a> {
    recorded: Option<&'a Recorded>,
    cached: bool,
    ordinal_known: bool,
}

fn stored_parts(
    domain: &str,
    entries: &[Entry],
    autostart: bool,
    how: StoredHow<'_>,
    run_line: Option<RunLineBody>,
    selectors: Option<&str>,
) -> Option<Parts> {
    let mut shown = if entries.is_empty() {
        Shown::none()
    } else {
        stored_shown(entries, how.recorded, how.cached, how.ordinal_known)?
    };
    if let Some(run_line) = run_line {
        shown.run_line = run_line;
    }
    let mut parts = assemble(domain, autostart, shown, selectors)?;
    if let Some(export) = entries.first().and_then(entry_export) {
        parts.export = export;
    }
    Some(parts)
}

fn grade_view(grade: Grade, score: Option<u8>) -> GradeView {
    GradeView {
        letter: grade_letter(grade).to_string(),
        score,
        tone: grade_tone(grade),
    }
}

/// `entry`'s score under its own profile version, `None` when the profile is unknown.
fn entry_score(entry: &Entry) -> Option<Score> {
    let Some(profile) = Profiles::get(entry.profile_version) else {
        tracing::warn!(entry = %entry.id, profile = entry.profile_version, "unknown profile version, no score");
        return None;
    };
    Some(score_run(&entry.run, profile))
}

/// The score shown under `entry`'s letter: the integer percentage, only when the run and its
/// score are complete and the engine's grade is the stored one. A differing grade is logged
/// whether or not the score is complete.
fn shown_score(entry: &Entry, score: &Score) -> Option<u8> {
    if score.grade != entry.grade {
        tracing::warn!(
            entry = %entry.id,
            stored = grade_letter(entry.grade),
            computed = grade_letter(score.grade),
            "stored grade differs from the engine's grade, no score"
        );
        return None;
    }
    if !entry.run.complete || !score.complete {
        return None;
    }
    let profile = Profiles::get(entry.profile_version)?;
    Some(views::score_number(
        score.overall_percentage,
        entry.grade,
        !score.hard_fail_checks.is_empty(),
        &profile.thresholds,
    ))
}

/// The banner of the finished run `done` shown under the letter `shown`, scored under its own
/// profile version.
fn done_banner(done: &Run, shown: Grade) -> Option<HardFailView> {
    let Some(profile) = Profiles::get(done.profile_version) else {
        tracing::warn!(
            profile = done.profile_version,
            "unknown profile version, no banner"
        );
        return None;
    };
    views::hard_fail_view(&score_run(done, profile), catalogue(), shown)
}

/// The first-run box: the explanation sentence and the line tones of the ghost trace.
struct FirstRun {
    text: String,
    cells: Vec<Tone>,
}

fn first_run(domain: &str, cells: Vec<Tone>) -> FirstRun {
    FirstRun {
        text: format!(
            "This is the first spectrum of {domain}. Re-check after your next change and netray stacks the runs into a history: a line that turns neutral is a fix, a line that lights up is a regression."
        ),
        cells,
    }
}

/// The diff block of the since section: the heading outside the box carries the title and the
/// grade line, so the box has none.
fn boxed(block: DiffBlockView) -> DiffBlockView {
    DiffBlockView {
        title: String::new(),
        summary: String::new(),
        ..block
    }
}

/// The grade line over the diff box, with the date of the run it is measured against in front
/// when one exists; `None` when the diff carries no grade change.
fn since_meta(grade: &[views::GradeStep], before: Option<&Entry>) -> Option<String> {
    if grade.is_empty() {
        return None;
    }
    let grade = views::grade_line(grade);
    Some(match before {
        Some(before) => format!(
            "{} \u{b7} grade {grade}",
            views::run_date(before.created_at)
        ),
        None => format!("grade {grade}"),
    })
}

/// The heading over the diff of a run that follows `earlier` stored runs.
fn since_heading(earlier: usize) -> Option<String> {
    (earlier > 0).then(|| format!("Since run {earlier}"))
}

/// One run behind the figure: when it was recorded, its grade, its place in the stored history
/// when known, what the legend's fixed entry names and its section grades.
pub(crate) struct FigureRow {
    created_at: i64,
    grade: Grade,
    no: Option<usize>,
    fixed: Option<String>,
    grades: BTreeMap<String, String>,
}

/// The figure rows of the stored `entries` (newest first); `numbered` is false when their place
/// in the history is unknown.
fn rows_of(entries: &[Entry], numbered: bool) -> Vec<FigureRow> {
    let total = entries.len();
    let mut rows: Vec<FigureRow> = entries
        .iter()
        .enumerate()
        .take(SHOWN)
        .map(|(j, e)| FigureRow {
            created_at: e.created_at,
            grade: e.grade,
            no: numbered.then(|| total - j),
            fixed: match (j + 1 < total, numbered) {
                (false, _) => None,
                (true, true) => fixed_since(total - j - 1),
                (true, false) => Some("the previous run".to_string()),
            },
            grades: section_grades(&e.run, Some(e.grade)),
        })
        .collect();
    unfix_oldest(&mut rows);
    rows
}

/// The oldest row of the layout window has no predecessor inside it, so no fixed tones were
/// computed for it and its legend names none.
fn unfix_oldest(rows: &mut [FigureRow]) {
    if let Some(oldest) = rows.last_mut() {
        oldest.fixed = None;
    }
}

/// The figure rows of the run `done` shown as `grade` over the stored `entries` before it.
pub(crate) fn done_rows(done: &Run, grade: Grade, entries: &[Entry]) -> Vec<FigureRow> {
    let mut rows = vec![FigureRow {
        created_at: now(),
        grade,
        no: None,
        fixed: fixed_since(entries.len()),
        grades: section_grades(done, Some(grade)),
    }];
    rows.extend(rows_of(entries, true).into_iter().take(SHOWN - 1));
    unfix_oldest(&mut rows);
    rows
}

/// What the key under the spectrum says: per run the predecessor a fix is measured against,
/// when there is one, the number of lines the spectrum draws, 0 when it draws none, and the run
/// the phone stepper names.
#[derive(Default)]
struct Legend {
    fixed: Vec<Option<String>>,
    checks: usize,
    newest: Option<StepRun>,
}

/// The run the phone stepper names: its place in the stored history when known, its date and
/// its grade letter.
struct StepRun {
    run: Option<usize>,
    date: String,
    grade: String,
}

/// The legend text naming run `earlier`, `None` when no run precedes.
fn fixed_since(earlier: usize) -> Option<String> {
    (earlier > 0).then(|| format!("run {earlier}"))
}

/// Each protocol's section grade of `run`: the section percentage of `score_run` through the
/// profile's thresholds; a section without a score has none. None at all when the profile is
/// unknown or the engine's grade is not the letter `shown`, as for the score and the hard-fail
/// sentence.
fn section_grades(run: &Run, shown: Option<Grade>) -> BTreeMap<String, String> {
    let Some(profile) = Profiles::get(run.profile_version) else {
        return BTreeMap::new();
    };
    let score = score_run(run, profile);
    if shown.is_some_and(|shown| score.grade != shown) {
        return BTreeMap::new();
    }
    score
        .sections
        .iter()
        .map(|(protocol, section)| {
            let grade = netray_engine::scoring::grade_for(profile, section.percentage);
            (
                protocol.as_str().to_string(),
                grade_letter(grade).to_string(),
            )
        })
        .collect()
}

/// What the figure of a run is made of: the plate, the trace, the legend and the run's line
/// tones, which the first-run box draws again.
pub(crate) struct Figure {
    plate: String,
    trace: String,
    legend: Legend,
    cells: Vec<Tone>,
}

/// The spectrum plate, the trace and the legend of the run `rows[0]`, laid out as `current`
/// over `older`; `live` names the bands the run on display measured, `stored` that the domain
/// has a history table to link.
pub(crate) fn figure_parts(
    domain: &str,
    current: &RunInput<'static>,
    older: &[RunInput<'static>],
    rows: &[FigureRow],
    live: &str,
    stored: bool,
) -> Option<Figure> {
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
    let newest = rows.first()?;
    let mut view = SpectrumView::from_layout(&layout, &label, 0).with_grades(&newest.grades);
    // Board FFirst draws no trace and no replay gutter: both need a history of two runs.
    if rows.len() > 1 {
        view = view.with_gutter(SpectrumGutter {
            run: newest.no,
            date: views::run_date(newest.created_at),
            grade: grade_letter(newest.grade).to_string(),
        });
        view = view.with_run_grades(
            rows.iter()
                .enumerate()
                .map(|(run, row)| SpectrumRunGrades {
                    run,
                    grades: row.grades.clone().into_iter().collect(),
                })
                .collect(),
        );
    }
    let plate = warn_render("spectrum", components::spectrum_live(&view, live))?;
    let inputs: Vec<views::TraceInput> = rows
        .iter()
        .map(|row| views::TraceInput {
            created_at: row.created_at,
            grade: row.grade,
            no: row.no,
        })
        .collect();
    let table_href = stored.then(|| format!("/{}/history", href_of(domain)));
    let trace = if rows.len() > 1 {
        warn_render(
            "trace",
            components::trace(&views::trace_view(&layout, &inputs, table_href)),
        )?
    } else {
        String::new()
    };
    let legend = Legend {
        fixed: if rows.len() > 1 {
            rows.iter().map(|row| row.fixed.clone()).collect()
        } else {
            Vec::new()
        },
        checks: layout.lines.len(),
        newest: Some(StepRun {
            run: newest.no,
            date: views::run_date(newest.created_at),
            grade: grade_letter(newest.grade).to_string(),
        }),
    };
    let cells = layout
        .runs
        .first()
        .map(|run| {
            run.tones
                .iter()
                .map(|t| components::line_tone(*t))
                .collect()
        })
        .unwrap_or_default();
    Some(Figure {
        plate,
        trace,
        legend,
        cells,
    })
}

/// What the latest of `entries` (newest first, not empty) shows.
fn stored_shown(
    entries: &[Entry],
    recorded: Option<&Recorded>,
    cached: bool,
    ordinal_known: bool,
) -> Option<Shown> {
    let entry = entries.first()?;
    let cat = catalogue();
    let score = entry_score(entry);
    let inputs: Vec<RunInput<'static>> = entries
        .iter()
        .take(SHOWN)
        .map(|e| views::layout_input_or_current(&e.run))
        .collect();
    let Figure {
        plate,
        trace,
        legend,
        cells,
    } = figure_parts(
        &entry.domain,
        &inputs[0],
        &inputs[1..],
        &rows_of(entries, ordinal_known),
        "",
        true,
    )?;
    let chosen: Vec<&Entry> = entries.iter().take(SHOWN).collect();
    let mut since: BTreeMap<CheckId, String> = BTreeMap::new();
    if ordinal_known {
        for check in &entry.run.checks {
            let statuses: Vec<Option<Status>> = entries
                .iter()
                .rev()
                .map(|e| {
                    e.run
                        .checks
                        .iter()
                        .find(|c| c.id == check.id)
                        .map(|c| c.status)
                })
                .collect();
            if let Some(text) = views::since_text(&statuses, 1) {
                since.insert(check.id.clone(), text);
            }
        }
    }
    let diff = match recorded {
        Some(recorded) => views::recorded_diff_view(recorded, Some(entry), cat),
        None => views::diff_view(entry.diff.as_ref(), cat),
    };
    let ordinal = ordinal_known.then_some(entries.len());
    let first = match ordinal {
        Some(n) => n == 1,
        None => matches!(recorded, Some(Recorded::First { .. })),
    };
    let diff_heading = match ordinal {
        Some(n) => since_heading(n - 1),
        None => Some("Since the previous run".to_string()),
    };
    let diff_meta = since_meta(&diff.grade, entries.get(1));
    Some(Shown {
        grade: Some(grade_view(
            entry.grade,
            score.as_ref().and_then(|s| shown_score(entry, s)),
        )),
        date: Some(views::run_date(entry.created_at)),
        run_line: RunLineBody::Title(views::run_title(entry, ordinal, cached)),
        share: entry.run.complete.then(|| entry.id.clone()),
        plate,
        trace,
        legend,
        sentences: sentences_of(chosen.iter().rev().copied()),
        refreshing: false,
        first_run: first.then(|| first_run(&entry.domain, cells)),
        diff_heading: if first { None } else { diff_heading },
        diff_meta: if first { None } else { diff_meta },
        diff: (!first).then(|| boxed(diff.block)),
        hard_fail: score
            .as_ref()
            .and_then(|s| views::hard_fail_view(s, cat, entry.grade)),
        groups: views::issues(
            &entry.run,
            cat,
            Links::Entry(&entry.id),
            Grouping::Owner,
            &since,
        ),
        clean: views::clean_count(&entry.run, cat),
        passed: views::passed_checks(&entry.run, cat),
        evidence: panels(&entry.run, &entry.domain, Some(&entry.id)),
        recorded: true,
        recheck: true,
    })
}

/// The figure of the finished run `done` over `entries`, its own row graded `grade`.
fn done_figure(domain: &str, done: &Run, grade: Grade, entries: &[Entry]) -> Option<Figure> {
    let current = views::layout_input_or_current(done);
    let older: Vec<RunInput<'static>> = entries
        .iter()
        .take(SHOWN - 1)
        .map(|e| views::layout_input_or_current(&e.run))
        .collect();
    figure_parts(
        domain,
        &current,
        &older,
        &done_rows(done, grade, entries),
        "",
        !entries.is_empty(),
    )
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
    let Figure {
        plate,
        trace,
        legend,
        ..
    } = done_figure(domain, done, grade, entries)?;
    let diff = views::recorded_diff_view(recorded, entries.first(), cat);
    Some(Shown {
        grade: Some(grade_view(grade, None)),
        date: Some(views::run_date(now())),
        run_line: RunLineBody::Line(evidence::incomplete_run_line()),
        share: None,
        plate,
        trace,
        legend,
        sentences: sentences_through(entries, done),
        refreshing: false,
        first_run: None,
        diff_heading: Some(since_heading(entries.len()).unwrap_or_else(|| "This run".to_string())),
        diff_meta: since_meta(&diff.grade, entries.first()),
        diff: Some(boxed(diff.block)),
        hard_fail: done_banner(done, grade),
        groups: views::issues(done, cat, Links::Live, Grouping::Owner, &BTreeMap::new()),
        clean: views::clean_count(done, cat),
        passed: views::passed_checks(done, cat),
        evidence: panels(done, domain, None),
        recorded: false,
        recheck: true,
    })
}

/// What a complete run of `domain` whose record failed shows: its own grade, "not recorded", no
/// share link.
fn unrecorded_shown(domain: &str, done: &Run, entries: &[Entry]) -> Option<Shown> {
    let cat = catalogue();
    let Figure {
        plate,
        trace,
        legend,
        ..
    } = done_figure(domain, done, done.grade, entries)?;
    Some(Shown {
        grade: Some(grade_view(done.grade, None)),
        date: Some(views::run_date(now())),
        run_line: RunLineBody::Line(RunLineView {
            parts: vec![views::NOT_RECORDED.to_string()],
        }),
        share: None,
        plate,
        trace,
        legend,
        sentences: sentences_through(entries, done),
        refreshing: false,
        first_run: None,
        diff_heading: Some(since_heading(entries.len()).unwrap_or_else(|| "This run".to_string())),
        diff_meta: None,
        diff: Some(boxed(views::not_recorded_view().block)),
        hard_fail: done_banner(done, done.grade),
        groups: views::issues(done, cat, Links::Live, Grouping::Owner, &BTreeMap::new()),
        clean: views::clean_count(done, cat),
        passed: views::passed_checks(done, cat),
        evidence: panels(done, domain, None),
        recorded: false,
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
        duration_ms: None,
    })
}

fn grouping_switch() -> GroupingSwitchView {
    let option = |value: &str, label: &str, pressed| GroupingOption {
        value: value.to_string(),
        label: label.to_string(),
        pressed,
    };
    GroupingSwitchView {
        label: "Group lines by".to_string(),
        options: vec![
            option("owner", "Owner", true),
            option("severity", "Severity", false),
            option("protocol", "Protocol", false),
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
    date: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "partials/result_share.html")]
struct SharePart<'a> {
    id: Option<&'a str>,
    domain: &'a str,
    snapshot: String,
    rerun: String,
    card: String,
    md_flat: String,
    html_flat: String,
    md_big: String,
    html_big: String,
}

/// The badge style the badge route reads from `?style=` besides its flat default.
const BADGE_STYLE_BIG: &str = "for-the-badge";

/// `text` for an attribute value or a textarea, with the named entities the markup elsewhere uses;
/// the share template prints the result unescaped.
fn html_escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn share_part<'a>(id: Option<&'a str>, domain: &'a str) -> SharePart<'a> {
    let origin = super::ORIGIN;
    let rerun = format!("{origin}/{domain}");
    let badge = format!("{origin}/badge/{domain}.svg");
    let md = |query: &str| format!("[![netray grade]({badge}{query})]({rerun})");
    let html = |query: &str| {
        format!(r#"<a href="{rerun}"><img src="{badge}{query}" alt="netray grade"></a>"#)
    };
    let html = |query: &str| html_escaped(&html(query));
    let big = format!("?style={BADGE_STYLE_BIG}");
    SharePart {
        id,
        domain,
        snapshot: format!("{origin}/r/{}", id.unwrap_or_default()),
        card: format!("{origin}/r/{}/og.png", id.unwrap_or_default()),
        md_flat: md(""),
        html_flat: html(""),
        md_big: md(&big),
        html_big: html(&big),
        rerun,
    }
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
    dkim_selectors: Option<&'a str>,
    button: &'a str,
    share: &'a str,
}

#[derive(Template)]
#[template(path = "partials/result_recheck_button.html")]
struct RecheckButtonPart {
    trigger: &'static str,
    trigger_label: &'static str,
}

#[derive(Template)]
#[template(path = "partials/result_spectrum.html")]
struct SpectrumPart<'a> {
    plate: &'a str,
    trace: &'a str,
    legend: &'a Legend,
    sentences: &'a [(String, String, String)],
    refreshing: bool,
    stepper: bool,
}

#[derive(Template)]
#[template(path = "partials/result_hard_fail.html")]
struct HardFailPart<'a> {
    banner: &'a Option<HardFailView>,
}

#[derive(Template)]
#[template(path = "partials/result_diff.html")]
struct DiffPart<'a> {
    first: &'a Option<FirstRun>,
    heading: &'a Option<String>,
    meta: &'a Option<String>,
    diff: &'a Option<DiffBlockView>,
}

#[derive(Template)]
#[template(path = "partials/result_issues.html")]
struct IssuesPart<'a> {
    switch: &'a GroupingSwitchView,
    groups: &'a [IssueGroup],
    count: usize,
    fails: usize,
    warns: usize,
    heading: String,
}

#[derive(Template)]
#[template(path = "partials/result_evidence.html")]
struct EvidencePart<'a> {
    protocol: &'static str,
    shown: bool,
    panel: &'a Option<evidence::PanelView>,
    selector_form: Option<&'a str>,
}

#[derive(Template)]
#[template(path = "partials/selector_form.html")]
struct SelectorFormPart<'a> {
    action: &'a str,
    value: Option<&'a str>,
}

/// The form that runs `action`'s domain with custom DKIM selectors, `value` the list it holds.
pub(crate) fn selector_form(action: &str, value: Option<&str>) -> Option<String> {
    warn_render(
        "selector-form",
        trimmed(&SelectorFormPart { action, value }),
    )
}

#[derive(Template)]
#[template(path = "partials/result_evidence_nav.html")]
struct EvidenceNavPart {
    tabs: Vec<TabView>,
}

#[derive(Template)]
#[template(path = "partials/result_evidence_meta.html")]
struct EvidenceMetaPart {
    recorded: bool,
}

#[derive(Template)]
#[template(path = "partials/result_passed.html")]
struct PassedPart<'a> {
    rows: &'a [CheckRowView],
    clean: usize,
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

/// What a line is called in the caption: its short label and its line code, "HSTS HTTP\u{b7}21";
/// empty when the catalogue does not know the check.
fn line_name(code: &str) -> String {
    let Ok(id) = CheckId::parse(code) else {
        return String::new();
    };
    let Some(spec) = catalogue().get(&id) else {
        return String::new();
    };
    match views::line_code(&id) {
        Some(line) => format!("{} {line}", spec.short),
        None => spec.short.clone(),
    }
}

fn spectrum_part(
    plate: &str,
    trace: &str,
    legend: &Legend,
    sentences: &[(String, String)],
    refreshing: bool,
) -> Option<String> {
    let named: Vec<(String, String, String)> = sentences
        .iter()
        .map(|(code, sentence)| (code.clone(), line_name(code), sentence.clone()))
        .collect();
    warn_render(
        "spectrum",
        trimmed(&SpectrumPart {
            plate,
            trace,
            legend,
            sentences: &named,
            refreshing,
            stepper: trace.matches("data-run=").count() >= 2,
        }),
    )
}

fn issues_part(groups: &[IssueGroup]) -> Option<String> {
    let switch = grouping_switch();
    let rows = || groups.iter().flat_map(|g| &g.rows);
    let fails = rows().filter(|r| r.status == "fail").count();
    let warns = rows().filter(|r| r.status == "warn").count();
    let count = fails + warns;
    let heading = if count == 1 {
        "1 line needs attention".to_string()
    } else {
        format!("{count} lines need attention")
    };
    warn_render(
        "issues",
        trimmed(&IssuesPart {
            switch: &switch,
            groups,
            count,
            fails,
            warns,
            heading,
        }),
    )
}

fn evidence_meta_part(recorded: bool) -> Option<String> {
    warn_render("evidence-meta", trimmed(&EvidenceMetaPart { recorded }))
}

/// One protocol's panel; `selector_form` follows it in a panel of its own, outside the element
/// the stream replaces.
fn evidence_part(
    protocol: Protocol,
    inner: &EvidenceInner,
    selector_form: Option<&str>,
) -> Option<String> {
    warn_render(
        "evidence",
        trimmed(&EvidencePart {
            protocol: protocol.as_str(),
            shown: inner.shown,
            panel: &inner.panel,
            selector_form,
        }),
    )
}

/// `body` of a run with the custom DKIM `selectors` (normalised, comma-joined): it names them
/// and that the run is not recorded.
fn with_selectors(body: RunLineBody, selectors: Option<&str>) -> RunLineBody {
    let Some(selectors) = selectors else {
        return body;
    };
    let extend = |mut view: RunLineView| {
        let noted = view.parts.iter().any(|p| p.contains(views::NOT_RECORDED));
        view.parts.retain(|p| p != views::NOT_RECORDED);
        view.parts
            .push(format!("DKIM selectors {}", selectors.replace(',', ", ")));
        if !noted || !view.parts.iter().any(|p| p.contains(views::NOT_RECORDED)) {
            view.parts.push(views::NOT_RECORDED.to_string());
        }
        view
    };
    match body {
        RunLineBody::Line(view) => RunLineBody::Line(extend(view)),
        RunLineBody::Title(view) => RunLineBody::Title(extend(view)),
        RunLineBody::Text(text) => RunLineBody::Line(extend(RunLineView { parts: vec![text] })),
        refusal @ RunLineBody::Refusal { .. } => refusal,
    }
}

/// `selectors` is the run's custom DKIM selector list (normalised): the recheck form's hidden
/// field, the run line and the selector form's value.
fn assemble(domain: &str, autostart: bool, shown: Shown, selectors: Option<&str>) -> Option<Parts> {
    let href = href_of(domain);
    let (trigger, trigger_label) = if shown.recheck {
        ("recheck", "Re-check now")
    } else {
        ("visit", "Check now")
    };
    let button = warn_render(
        "recheck-button",
        trimmed(&RecheckButtonPart {
            trigger,
            trigger_label,
        }),
    )?;
    // A stored entry shown under a selector page is not the selector run: the line says what is
    // shown until that run ends, instead of relabelling the stored run.
    let line = match selectors {
        Some(list) if shown.recorded => RunLineBody::Line(RunLineView {
            parts: vec![
                format!("DKIM selectors {}", list.replace(',', ", ")),
                views::NOT_RECORDED.to_string(),
                "the stored run shows until this run ends".to_string(),
            ],
        }),
        _ => with_selectors(shown.run_line, selectors),
    };
    let run_line = run_line_part(&line)?;
    let progress = progress_part("")?;
    let grade = warn_render(
        "result-grade",
        trimmed(&GradePart {
            grade: &shown.grade,
            date: &shown.date,
        }),
    )?;
    let share = warn_render(
        "result-share",
        trimmed(&share_part(shown.share.as_deref(), domain)),
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
            dkim_selectors: selectors,
            button: &button,
            share: &share,
        }),
    )?;
    let diff = warn_render(
        "diff",
        trimmed(&DiffPart {
            first: &shown.first_run,
            heading: &shown.diff_heading,
            meta: &shown.diff_meta,
            diff: &shown.diff,
        }),
    )?;
    let hard_fail = warn_render(
        "hard-fail",
        trimmed(&HardFailPart {
            banner: &shown.hard_fail,
        }),
    )?;
    let passed = warn_render(
        "passed",
        trimmed(&PassedPart {
            rows: &shown.passed,
            clean: shown.clean,
        }),
    )?;
    let form = selector_form(&format!("/{href}"), selectors)?;
    let evidence = PROTOCOLS
        .iter()
        .zip(&shown.evidence)
        .map(|(p, inner)| evidence_part(*p, inner, (*p == Protocol::Email).then_some(&*form)))
        .collect::<Option<Vec<_>>>()?;
    let email_fragment = PROTOCOLS
        .iter()
        .zip(&shown.evidence)
        .find(|(p, _)| **p == Protocol::Email)
        .and_then(|(p, inner)| evidence_part(*p, inner, None))?;
    let tabs = PROTOCOLS
        .iter()
        .zip(&shown.evidence)
        .map(|(p, inner)| TabView {
            protocol: p.as_str(),
            label: tab_label(*p),
            current: inner.shown,
        })
        .collect();
    let nav = warn_render("evidence-nav", trimmed(&EvidenceNavPart { tabs }))?;
    let meta = evidence_meta_part(shown.recorded)?;
    Some(Parts {
        header,
        hard_fail,
        grade,
        run_line,
        progress,
        share,
        button,
        recheck: shown.recheck,
        spectrum: spectrum_part(
            &shown.plate,
            &shown.trace,
            &shown.legend,
            &shown.sentences,
            shown.refreshing,
        )?,
        diff,
        issues: issues_part(&shown.groups)?,
        meta,
        nav,
        evidence,
        email_fragment,
        passed,
        export: EXPORT_PLACEHOLDER.to_string(),
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
    help: PageHelp,
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
    legend: Vec<(&'static str, &'static str, &'static str, &'static str)>,
}
