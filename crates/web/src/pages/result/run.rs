//! `POST /{domain}/run`: the recheck form, answered as a stream of page fragments for the
//! stream island or as a redirect for a plain form post.

use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::connect_info::ConnectInfo;
use axum::extract::rejection::PathRejection;
use axum::extract::{Form, FromRequest, Path, Request, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, LOCATION, RETRY_AFTER};
use axum::response::sse::Event;
use axum::response::{IntoResponse, Response, Sse};
use futures::StreamExt;
use netray_engine::SectionEvent;
use netray_engine::catalogue::catalogue;
use netray_model::Protocol;
use netray_model::run::Run;
use netray_store::{Entry, HistoryStore, Recorded};
use serde::Deserialize;

use super::views::{self, Grouping, Links};
use super::{
    EvidenceInner, PROTOCOLS, Parts, ResultState, RunLineBody, SHOWN, StoredHow, assemble,
    domain_segment, done_rows, evidence_meta_part, evidence_part, figure_parts, full_page, href_of,
    is_fresh, issues_part, not_stored_shown, progress_part, recorded_entry, resolve, run_line_part,
    sentences_of, spectrum_part, stored_parts, unrecorded_shown, with_selectors,
};
use crate::api::{RunEvent, RunEvents, RunRefusal, RunStart, RunTrigger, wants_event_stream};
use crate::client::client_ip;
use crate::components::{self, ProgressRayView, RaySegment, RunLineView};
use crate::evidence;
use crate::pages::errors::selectors_refused_page;
use crate::pages::headers::secure;
use crate::pages::server_error;
use crate::spectrum::RunInput;
use crate::stream::{end, fragment};

#[derive(Deserialize)]
struct RunForm {
    trigger: String,
    /// Custom DKIM selectors, comma-separated (spec v2-dkim-legacy-urls requirement 5).
    #[serde(default)]
    dkim_selectors: Option<String>,
}

type Events = futures::stream::BoxStream<'static, Result<Event, Infallible>>;

pub(super) async fn run(
    State(state): State<ResultState>,
    path: Result<Path<String>, PathRejection>,
    request: Request,
) -> Response {
    let Some(segment) = domain_segment(path) else {
        return (state.fallback)(request).await;
    };
    let domain = match resolve(&state, &segment, "/run", false) {
        Ok(domain) => domain,
        Err(response) => return *response,
    };
    let (parts, body) = request.into_parts();
    let headers = parts.headers.clone();
    let peer = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0);
    let client = client_ip(state.ip_extractor(), &headers, peer);
    let prefetch = netray_common::prefetch::is_prefetch(&headers);
    let sse = wants_event_stream(&headers);

    let (trigger, raw_selectors) =
        match Form::<RunForm>::from_request(Request::from_parts(parts, body), &()).await {
            Ok(Form(form)) => match form.trigger.as_str() {
                "visit" => (RunTrigger::Visit, form.dkim_selectors),
                "recheck" => (RunTrigger::Recheck, form.dkim_selectors),
                _ => return secure(StatusCode::BAD_REQUEST.into_response()),
            },
            Err(_) => return secure(StatusCode::BAD_REQUEST.into_response()),
        };
    let selectors = match crate::selectors::parse_dkim_selectors(raw_selectors.as_deref()) {
        Ok(selectors) => selectors,
        Err(reason) => return selectors_refused_page(&domain, &reason),
    };
    let joined = (!selectors.is_empty()).then(|| selectors.join(","));

    // A run with custom selectors asks what the stored head does not answer.
    if trigger == RunTrigger::Visit && selectors.is_empty() {
        match state.store.entries(&domain).await {
            Ok(entries) if is_fresh(&entries, state.ttl_seconds) => {
                return stored_answer(&domain, &entries, sse);
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(domain = %domain, error = %e, "history lookup failed");
                return server_error();
            }
        }
    }

    match state
        .runs
        .start_with(&domain, client, trigger, prefetch, &selectors)
        .await
    {
        RunStart::NotStarted => secure(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                [(CACHE_CONTROL, "no-store")],
            )
                .into_response(),
        ),
        RunStart::Refused(refusal) => {
            refused(&state, &domain, &refusal, sse, joined.as_deref()).await
        }
        RunStart::Admitted(events) if sse => stream(&state, domain, events, joined).await,
        RunStart::Admitted(events) if joined.is_some() => {
            finished_page(&state, &domain, events, joined.as_deref()).await
        }
        RunStart::Admitted(mut events) => {
            while events.next().await.is_some() {}
            see_other(&domain)
        }
    }
}

fn see_other(domain: &str) -> Response {
    secure(
        (
            StatusCode::SEE_OTHER,
            [(LOCATION, format!("/{}", href_of(domain)))],
        )
            .into_response(),
    )
}

fn sse_response(status: StatusCode, events: Vec<Event>) -> Response {
    let stream: Events = futures::stream::iter(events.into_iter().map(Ok)).boxed();
    secure((status, Sse::new(stream)).into_response())
}

/// The end fragments of the stored history, for a visit whose head is still fresh; the head's
/// export comes with them, since the page may have been rendered from an older entry.
fn stored_answer(domain: &str, entries: &[Entry], sse: bool) -> Response {
    if !sse {
        return see_other(domain);
    }
    let how = StoredHow {
        recorded: None,
        cached: true,
        ordinal_known: true,
    };
    match stored_parts(domain, entries, false, how, None, None) {
        Some(parts) => {
            let mut events: Vec<Event> = parts
                .fragments()
                .into_iter()
                .map(|(id, html)| fragment(&id, html))
                .collect();
            events.push(end());
            sse_response(StatusCode::OK, events)
        }
        None => server_error(),
    }
}

async fn refused(
    state: &ResultState,
    domain: &str,
    refusal: &RunRefusal,
    sse: bool,
    selectors: Option<&str>,
) -> Response {
    let (lead, retry) = views::refusal_line(refusal);
    let body = RunLineBody::Refusal { lead, retry };
    let mut response = if sse {
        match run_line_part(&body) {
            Some(html) => sse_response(
                StatusCode::TOO_MANY_REQUESTS,
                vec![fragment("run-line", &html), end()],
            ),
            None => return server_error(),
        }
    } else {
        let entries = match state.store.entries(domain).await {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(domain = %domain, error = %e, "history lookup failed");
                return server_error();
            }
        };
        full_page(
            state,
            domain,
            &entries,
            StatusCode::TOO_MANY_REQUESTS,
            "no-store",
            Some(body),
            selectors,
        )
    };
    response.headers_mut().insert(RETRY_AFTER, retry.into());
    response
}

/// The result page of a run read to its end, for a selector run posted without the stream island:
/// the run is not recorded, so a redirect to the stored page would not show it.
async fn finished_page(
    state: &ResultState,
    domain: &str,
    mut events: RunEvents,
    selectors: Option<&str>,
) -> Response {
    // The run is already admitted: a failed history read still shows it, as the stream does.
    let entries = state
        .store
        .entries(domain)
        .await
        .inspect_err(|e| tracing::warn!(domain = %domain, error = %e, "history lookup failed"))
        .unwrap_or_default();
    let mut done = None;
    let mut recorded = None;
    while let Some(event) = events.next().await {
        match event {
            RunEvent::Section(_) => {}
            RunEvent::Done(run) => done = Some(run),
            RunEvent::Recorded(r) => {
                recorded = r;
                break;
            }
        }
    }
    let Some(done) = done else {
        return server_error();
    };
    let state_word = super::state_word(&entries, state.ttl_seconds);
    match done_parts(&state.store, domain, &entries, &done, recorded, selectors).await {
        Some(parts) => super::result_page(StatusCode::OK, "no-store", domain, state_word, parts),
        None => server_error(),
    }
}

/// The parts of a finished run over the history `entries` as it was when the run started.
async fn done_parts(
    store: &HistoryStore,
    domain: &str,
    entries: &[Entry],
    done: &Run,
    recorded: Option<Recorded>,
    selectors: Option<&str>,
) -> Option<Parts> {
    let not_stored = |recorded: &Recorded| {
        not_stored_shown(domain, done, recorded, entries)
            .and_then(|shown| assemble(domain, false, shown, selectors))
    };
    match recorded {
        None if done.complete => unrecorded_shown(domain, done, entries)
            .and_then(|shown| assemble(domain, false, shown, selectors)),
        None => not_stored(&Recorded::NotStored { diff: None }),
        Some(Recorded::NotStored { diff }) => not_stored(&Recorded::NotStored { diff }),
        Some(stored) => Live::stored(store, domain, done, &stored, selectors).await,
    }
}

/// The state of one streamed run between events.
struct Live {
    events: RunEvents,
    queue: VecDeque<Event>,
    ended: bool,
    store: Arc<HistoryStore>,
    domain: String,
    /// The history as it was when the run started, newest first.
    entries: Vec<Entry>,
    sections: Vec<SectionEvent>,
    done: Option<Run>,
    /// The run's custom DKIM selectors, normalised.
    selectors: Option<String>,
}

async fn stream(
    state: &ResultState,
    domain: String,
    events: RunEvents,
    selectors: Option<String>,
) -> Response {
    let entries = state
        .store
        .entries(&domain)
        .await
        .inspect_err(|e| tracing::warn!(domain = %domain, error = %e, "history lookup failed"))
        .unwrap_or_default();
    let mut live = Live {
        events,
        queue: VecDeque::new(),
        ended: false,
        store: state.store.clone(),
        domain,
        entries,
        sections: Vec::new(),
        done: None,
        selectors,
    };
    live.start();
    let stream: Events = futures::stream::unfold(live, |mut live| async move {
        loop {
            if let Some(event) = live.queue.pop_front() {
                return Some((Ok(event), live));
            }
            if live.ended {
                return None;
            }
            match live.events.next().await {
                Some(RunEvent::Section(section)) => live.section(section),
                Some(RunEvent::Done(run)) => live.done = Some(run),
                Some(RunEvent::Recorded(recorded)) => {
                    live.finish(recorded).await;
                    live.ended = true;
                }
                None => {
                    live.queue.push_back(end());
                    live.ended = true;
                }
            }
        }
    })
    .boxed();
    secure((StatusCode::OK, Sse::new(stream)).into_response())
}

fn ray(run: &Run) -> Option<String> {
    let view = ProgressRayView {
        label: "Check progress".to_string(),
        segments: PROTOCOLS
            .iter()
            .zip(views::segment_states(run))
            .map(|(p, state)| RaySegment {
                label: p.as_str().to_string(),
                state,
            })
            .collect(),
    };
    components::progress_ray(&view)
        .inspect_err(|e| tracing::warn!(error = %e, "progress ray render failed"))
        .ok()
}

impl Live {
    fn push(&mut self, id: &str, html: Option<String>) {
        if let Some(html) = html {
            self.queue.push_back(fragment(id, &html));
        }
    }

    fn progress(&mut self, run: &Run) {
        let html = ray(run).and_then(|ray| progress_part(&ray));
        self.push("progress", html);
    }

    /// The figure of the run so far over the history before it.
    fn figure(&mut self, run: &Run) {
        let current = views::live_input(self.entries.first().map(|e| &e.run), run);
        let older: Vec<RunInput<'static>> = self
            .entries
            .iter()
            .take(SHOWN - 1)
            .map(|e| views::layout_input_or_current(&e.run))
            .collect();
        let rows = done_rows(run, netray_model::Grade::Incomplete, &self.entries);
        let live = run
            .sections
            .iter()
            .map(|s| s.protocol.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let sentences = sentences_of(self.entries.iter().take(SHOWN - 1).rev());
        let stored = !self.entries.is_empty();
        let html =
            figure_parts(&self.domain, &current, &older, &rows, &live, stored).and_then(|figure| {
                spectrum_part(
                    &figure.plate,
                    &figure.trace,
                    &figure.legend,
                    &sentences,
                    true,
                )
            });
        self.push("spectrum", html);
    }

    fn start(&mut self) {
        let run = views::live_run(&[]);
        let line = RunLineBody::Line(RunLineView {
            parts: vec!["running".to_string()],
        });
        let html = run_line_part(&with_selectors(line, self.selectors.as_deref()));
        self.push("run-line", html);
        self.progress(&run);
        self.figure(&run);
    }

    fn section(&mut self, section: SectionEvent) {
        let protocol: Protocol = section.protocol;
        self.sections.push(section);
        let run = views::live_run(&self.sections);
        self.progress(&run);
        self.figure(&run);
        let groups = views::issues(
            &run,
            catalogue(),
            Links::Live,
            Grouping::Owner,
            &BTreeMap::new(),
        );
        self.push("issues", issues_part(&groups));
        let inner = EvidenceInner {
            shown: false,
            panel: Some(evidence::panel(&run, protocol, &self.domain, None)),
        };
        let html = evidence_part(protocol, &inner, None);
        self.push(&format!("evidence-{}", protocol.as_str()), html);
        if self.sections.len() == 1 {
            self.push("evidence-meta", evidence_meta_part(false));
        }
    }

    /// The parts of the history with the recorded entry as its head; when the re-read does not
    /// return it, the finished run stands in for it.
    async fn stored(
        store: &HistoryStore,
        domain: &str,
        done: &Run,
        recorded: &Recorded,
        selectors: Option<&str>,
    ) -> Option<Parts> {
        let mut entries = store
            .entries(domain)
            .await
            .inspect_err(|e| tracing::warn!(domain = %domain, error = %e, "history lookup failed"))
            .unwrap_or_default();
        let entry = recorded_entry(domain, done, recorded)?;
        let stand_in = entries.first().map(|e| &e.id) != Some(&entry.id);
        if stand_in {
            entries.insert(0, entry);
        }
        let ordinal_known = !stand_in || matches!(recorded, Recorded::First { .. });
        let how = StoredHow {
            recorded: Some(recorded),
            cached: false,
            ordinal_known,
        };
        stored_parts(domain, &entries, false, how, None, selectors)
    }

    async fn finish(&mut self, recorded: Option<Recorded>) {
        let Some(done) = self.done.take() else {
            self.queue.push_back(end());
            return;
        };
        let parts = done_parts(
            &self.store,
            &self.domain,
            &self.entries,
            &done,
            recorded,
            self.selectors.as_deref(),
        )
        .await;
        if let Some(parts) = parts {
            for (id, html) in parts.fragments() {
                self.queue.push_back(fragment(&id, html));
            }
        }
        self.queue.push_back(end());
    }
}
