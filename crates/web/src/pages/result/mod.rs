//! The result page: the views it renders from a stored entry or a fresh run, and the router of
//! `/`, `/{domain}`, `/{domain}/history` and `/{domain}/run`.

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
use netray_model::{CheckId, Protocol, Status};
use netray_store::{Entry, HistoryStore};
use percent_encoding::utf8_percent_encode;

use self::views::{Grouping, IssueGroup, Links};
use super::errors::{LOCATION_SET, query_view, refused_page};
use super::headers::secure;
use super::{Frame, page, query_redirect, server_error};
use crate::api::DomainRuns;
use crate::components::{
    self, CheckRowView, GradeView, GroupingOption, GroupingSwitchView, QueryInputView, RunLineView,
    SpectrumView, TraceView,
};
use crate::entries::view::{grade_letter, grade_tone};
use crate::evidence::entry_run_line;
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
        .route("/{domain}/run", post(run).fallback(wrong_method))
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

async fn run(
    State(state): State<ResultState>,
    path: Result<Path<String>, PathRejection>,
    request: Request,
) -> Response {
    let Some(segment) = domain_segment(path) else {
        return (state.fallback)(request).await;
    };
    match resolve(&state, &segment, "/run", false) {
        Err(response) => *response,
        Ok(_) => secure(StatusCode::NOT_IMPLEMENTED.into_response()),
    }
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
    let mut shown: Vec<(&Entry, RunInput<'static>)> = entries
        .iter()
        .map(|e| (e, views::layout_input_or_current(&e.run)))
        .collect();
    shown.truncate(SHOWN);

    let (current, rest) = match shown.split_first() {
        Some((current, rest)) => (Some(current), rest),
        None => (None, &[][..]),
    };
    let fresh = current.is_some_and(|(e, _)| {
        now().saturating_sub(e.last_seen_at) < i64::try_from(state.ttl_seconds).unwrap_or(i64::MAX)
    });
    let view = match current {
        Some((entry, input)) => {
            let older: Vec<RunInput<'static>> = rest.iter().map(|(_, i)| i.clone()).collect();
            let chosen: Vec<&Entry> = shown.iter().map(|(e, _)| *e).collect();
            match entry_view(entry, input, &older, &chosen) {
                Some(view) => Some(view),
                None => return server_error(),
            }
        }
        None => None,
    };
    let state_word = match (&view, fresh) {
        (None, _) => "none",
        (Some(_), true) => "fresh",
        (Some(_), false) => "stale",
    };
    let (trigger, trigger_label) = if view.is_some() {
        ("recheck", "re-check now")
    } else {
        ("visit", "check now")
    };
    page(
        StatusCode::OK,
        "no-cache",
        &ResultPage {
            frame: Frame::new(
                &format!("{domain} - netray"),
                "Every check of one domain, in one spectrum.",
                &format!("/{}", href_of(&domain)),
            ),
            href: href_of(&domain),
            state: state_word,
            autostart: state_word != "fresh",
            trigger,
            trigger_label,
            switch: grouping_switch(),
            evidence: evidence_items(current.map(|(e, _)| *e)),
            shown: view,
            domain,
        },
    )
}

/// The page parts of the latest stored entry.
struct EntryView {
    grade: GradeView,
    run_line: RunLineView,
    share: Option<String>,
    spectrum: String,
    trace: TraceView,
    diff: crate::components::DiffBlockView,
    groups: Vec<IssueGroup>,
    passed: Vec<CheckRowView>,
}

/// `entries` is every shown entry, newest first, `entry` the first of them.
fn entry_view(
    entry: &Entry,
    input: &RunInput<'static>,
    older: &[RunInput<'static>],
    entries: &[&Entry],
) -> Option<EntryView> {
    let cat = catalogue();
    let width = (cat.entries().len() as f64 * spectrum::line_pitch_min()) as u32;
    let layout = match spectrum::layout(input, older, cat, width) {
        Ok(layout) => layout,
        Err(e) => {
            tracing::warn!(error = ?e, "spectrum layout failed");
            return None;
        }
    };
    let label = format!("Check results of {}", entry.domain);
    let spectrum = match components::spectrum(&SpectrumView::from_layout(&layout, &label, 0)) {
        Ok(html) => html,
        Err(e) => {
            tracing::warn!(error = %e, "spectrum render failed");
            return None;
        }
    };
    let mut since: BTreeMap<CheckId, String> = BTreeMap::new();
    for check in &entry.run.checks {
        let points: Vec<(i64, Option<Status>)> = entries
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
    Some(EntryView {
        grade: GradeView {
            letter: grade_letter(entry.grade).to_string(),
            score: None,
            tone: grade_tone(entry.grade),
        },
        run_line: entry_run_line(entry.created_at, &entry.id),
        share: entry.run.complete.then(|| entry.id.clone()),
        spectrum,
        trace: views::trace_view(&layout, entries),
        diff: views::diff_view(entry.diff.as_ref(), cat).block,
        groups: views::issues(
            &entry.run,
            cat,
            Links::Entry(&entry.id),
            Grouping::Owner,
            &since,
        ),
        passed: views::passed_checks(&entry.run, cat),
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

fn evidence_items(entry: Option<&Entry>) -> Vec<EvidenceItem> {
    PROTOCOLS
        .iter()
        .map(|p| EvidenceItem {
            protocol: p.as_str(),
            href: entry.map(|e| format!("/r/{}/{}", e.id, p.as_str())),
            outcome: entry.map_or_else(String::new, |e| views::section_outcome(&e.run, *p)),
        })
        .collect()
}

struct EvidenceItem {
    protocol: &'static str,
    href: Option<String>,
    outcome: String,
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
    href: String,
    state: &'static str,
    autostart: bool,
    trigger: &'static str,
    trigger_label: &'static str,
    switch: GroupingSwitchView,
    evidence: Vec<EvidenceItem>,
    shown: Option<EntryView>,
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
