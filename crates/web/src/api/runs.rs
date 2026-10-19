//! The seam between the API and the run machinery (spec v2-api requirement 1).
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;

use axum::Extension;
use axum::Json;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use futures::future::BoxFuture;
use futures::{Stream, StreamExt};
use netray_common::error::ErrorResponse;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::Protocol;
use netray_model::diff::Diff;
use netray_model::evidence::Block;
use netray_model::run::{Run, RunCheck, SectionKind};
use netray_store::Recorded;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::error::V1Error;
use super::limit::client;
use super::state::ApiState;

pub trait DomainRuns: Send + Sync + 'static {
    /// The normalised domain, or the refusal.
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal>;

    fn start<'a>(
        &'a self,
        domain: &'a str,
        client: IpAddr,
        trigger: RunTrigger,
        prefetch: bool,
    ) -> BoxFuture<'a, RunStart>;

    /// `start` with the run's DKIM selectors (spec v2-dkim-legacy-urls): an empty list is a plain
    /// run. A run with selectors answers a different question than the stored head: it never
    /// joins, is never joined and is not recorded. The default ignores the list and exists for
    /// test stubs; every production implementation overrides it
    /// (`tests/repo/test_domain_runs_selectors.sh`).
    fn start_with<'a>(
        &'a self,
        domain: &'a str,
        client: IpAddr,
        trigger: RunTrigger,
        prefetch: bool,
        dkim_selectors: &'a [String],
    ) -> BoxFuture<'a, RunStart> {
        let _ = dkim_selectors;
        self.start(domain, client, trigger, prefetch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainRefusal {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTrigger {
    Visit,
    Recheck,
    Api,
}

pub enum RunStart {
    Admitted(RunEvents),
    Refused(RunRefusal),
    NotStarted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRefusal {
    pub reason: RefusalReason,
    pub retry_after_s: u64,
    pub last_run_ago_s: Option<u64>,
    pub last_incomplete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalReason {
    Throttle,
    Budget,
    IpLimit,
    GlobalLimit,
    Slots,
}

pub type RunEvents = Pin<Box<dyn Stream<Item = RunEvent> + Send>>;

pub enum RunEvent {
    Section(netray_engine::SectionEvent),
    Done(netray_model::run::Run),
    Recorded(Option<netray_store::Recorded>),
}

/// What the history did with a finished run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum RecordedKind {
    First,
    Identical,
    Changed,
    NotStored,
}

/// The answer of a fresh run: the run, how the history recorded it, and the diff when it has one.
#[derive(Serialize, ToSchema)]
pub(super) struct RunAnswer {
    recorded: RecordedKind,
    entry_id: Option<String>,
    run: Run,
    diff: Option<Diff>,
}

impl RunAnswer {
    fn new(run: Run, recorded: Option<Recorded>) -> Self {
        let (recorded, entry_id, diff) = match recorded {
            Some(Recorded::First { id }) => (RecordedKind::First, Some(id), None),
            Some(Recorded::Identical { id }) => (RecordedKind::Identical, Some(id), None),
            Some(Recorded::Changed { id, diff }) => (RecordedKind::Changed, Some(id), Some(diff)),
            Some(Recorded::NotStored { diff }) => (RecordedKind::NotStored, None, diff),
            None => (RecordedKind::NotStored, None, None),
        };
        Self {
            recorded,
            entry_id,
            run,
            diff,
        }
    }
}

/// One finished section of a running check, as the `section` event of the stream.
#[derive(Serialize, ToSchema)]
pub(super) struct RunSectionEvent {
    protocol: Protocol,
    outcome: SectionKind,
    reason: Option<String>,
    checks: Vec<RunCheck>,
    blocks: Vec<Block>,
}

impl From<SectionEvent> for RunSectionEvent {
    fn from(event: SectionEvent) -> Self {
        let SectionEvent { protocol, outcome } = event;
        let (outcome, reason, checks, blocks) = match outcome {
            SectionOutcome::Measured { checks, evidence } => {
                let checks = checks
                    .into_iter()
                    .map(|c| RunCheck {
                        id: c.id,
                        status: c.status,
                        findings: c.findings,
                        evidence: c.evidence,
                    })
                    .collect();
                (SectionKind::Measured, None, checks, evidence)
            }
            SectionOutcome::NotApplicable { reason } => {
                (SectionKind::NotApplicable, Some(reason), vec![], vec![])
            }
            SectionOutcome::Incomplete { reason } => {
                (SectionKind::Incomplete, Some(reason), vec![], vec![])
            }
            SectionOutcome::TimedOut => (SectionKind::TimedOut, None, vec![], vec![]),
        };
        Self {
            protocol,
            outcome,
            reason,
            checks,
            blocks,
        }
    }
}

fn refusal_error(refusal: RunRefusal) -> V1Error {
    let retry_after_secs = refusal.retry_after_s;
    match refusal.reason {
        RefusalReason::Throttle => V1Error::Throttled { retry_after_secs },
        RefusalReason::Budget => V1Error::BudgetExhausted { retry_after_secs },
        RefusalReason::IpLimit | RefusalReason::GlobalLimit => {
            V1Error::RateLimited { retry_after_secs }
        }
        RefusalReason::Slots => V1Error::Busy { retry_after_secs },
    }
}

pub(crate) fn wants_event_stream(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .any(|t| {
            t.split(';')
                .next()
                .is_some_and(|m| m.trim().eq_ignore_ascii_case("text/event-stream"))
        })
}

fn sse_event(name: &str, data: &impl Serialize) -> Result<Event, axum::Error> {
    Event::default().event(name).json_data(data)
}

struct Relay {
    events: RunEvents,
    run: Option<Run>,
    finished: bool,
}

fn relay(events: RunEvents) -> impl Stream<Item = Result<Event, axum::Error>> {
    let state = Relay {
        events,
        run: None,
        finished: false,
    };
    futures::stream::unfold(state, |mut state| async move {
        if state.finished {
            return None;
        }
        while let Some(event) = state.events.next().await {
            match event {
                RunEvent::Section(section) => {
                    let out = sse_event("section", &RunSectionEvent::from(section));
                    return Some((out, state));
                }
                RunEvent::Done(run) => state.run = Some(run),
                RunEvent::Recorded(recorded) => {
                    state.finished = true;
                    let run = state.run.take()?;
                    let out = sse_event("done", &RunAnswer::new(run, recorded));
                    return Some((out, state));
                }
            }
        }
        None
    })
}

async fn answer(mut events: RunEvents) -> Result<Json<RunAnswer>, V1Error> {
    let mut run = None;
    while let Some(event) = events.next().await {
        match event {
            RunEvent::Section(_) => {}
            RunEvent::Done(done) => run = Some(done),
            RunEvent::Recorded(recorded) => {
                if let Some(run) = run {
                    return Ok(Json(RunAnswer::new(run, recorded)));
                }
                break;
            }
        }
    }
    Err(V1Error::Internal("the run ended without an answer".into()))
}

/// The query of `POST /domains/{domain}/runs`.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct RunQuery {
    /// Custom DKIM selectors, comma-separated: 1 to 5, each 1-63 of `[A-Za-z0-9-]`; empty means
    /// none. A run with selectors is never joined, never answered from the history and never
    /// recorded (`recorded: not_stored`).
    #[param(example = "google,selector1")]
    dkim_selectors: Option<String>,
}

/// Start a fresh run of a domain and answer with it; takes no body.
#[utoipa::path(
    post,
    path = "/domains/{domain}/runs",
    tag = "runs",
    params(("domain" = String, Path, description = "The domain to check"), RunQuery),
    responses(
        (
            status = 200,
            description = "The run and how the history recorded it; with `Accept: text/event-stream`, `section` events as they arrive, then one `done` event carrying the same answer",
            content(
                (RunAnswer = "application/json"),
                (RunSectionEvent = "text/event-stream"),
            )
        ),
        (status = 400, description = "Not a valid domain, or an invalid `dkim_selectors` list", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Throttled, budget spent, rate limited or no free run slot", body = ErrorResponse),
        (status = 503, description = "A prefetch does not start a run", body = ErrorResponse),
    )
)]
pub(super) async fn start_run(
    State(state): State<ApiState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    domain: Result<Path<String>, PathRejection>,
    query: Result<Query<RunQuery>, QueryRejection>,
) -> Result<Response, V1Error> {
    let Path(raw) = domain.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    let domain = state
        .runs()
        .validate(&raw)
        .map_err(|refusal| V1Error::DomainInvalid(refusal.message))?;
    let Query(RunQuery { dkim_selectors }) =
        query.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    let selectors = crate::selectors::parse_dkim_selectors(dkim_selectors.as_deref())
        .map_err(V1Error::InvalidInput)?;
    let client = client(
        &state,
        &headers,
        peer.map(|Extension(ConnectInfo(addr))| addr),
    );
    let prefetch = netray_common::prefetch::is_prefetch(&headers);
    match state
        .runs()
        .start_with(&domain, client, RunTrigger::Api, prefetch, &selectors)
        .await
    {
        RunStart::Refused(refusal) => Err(refusal_error(refusal)),
        RunStart::NotStarted => Err(V1Error::PrefetchRefused),
        RunStart::Admitted(events) if wants_event_stream(&headers) => {
            Ok(Sse::new(relay(events)).into_response())
        }
        RunStart::Admitted(events) => Ok(answer(events).await?.into_response()),
    }
}
