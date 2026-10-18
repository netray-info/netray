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
    EvidenceInner, PROTOCOLS, Parts, ResultState, RunLineBody, SHOWN, Shown, assemble,
    domain_segment, evidence_part, figure_parts, full_page, href_of, is_fresh, issues_part,
    not_stored_shown, progress_part, recorded_entry, resolve, run_line_part, sentences_of,
    spectrum_part, stored_parts, unrecorded_shown,
};
use crate::api::{RunEvent, RunEvents, RunRefusal, RunStart, RunTrigger, wants_event_stream};
use crate::client::client_ip;
use crate::components::{self, ProgressRayView, RaySegment, RunLineView};
use crate::evidence;
use crate::pages::headers::secure;
use crate::pages::server_error;
use crate::spectrum::RunInput;
use crate::stream::{end, fragment};

#[derive(Deserialize)]
struct RunForm {
    trigger: String,
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

    let trigger = match Form::<RunForm>::from_request(Request::from_parts(parts, body), &()).await {
        Ok(Form(form)) => match form.trigger.as_str() {
            "visit" => RunTrigger::Visit,
            "recheck" => RunTrigger::Recheck,
            _ => return secure(StatusCode::BAD_REQUEST.into_response()),
        },
        Err(_) => return secure(StatusCode::BAD_REQUEST.into_response()),
    };

    if trigger == RunTrigger::Visit {
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

    match state.runs.start(&domain, client, trigger, prefetch).await {
        RunStart::NotStarted => secure(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                [(CACHE_CONTROL, "no-store")],
            )
                .into_response(),
        ),
        RunStart::Refused(refusal) => refused(&state, &domain, &refusal, sse).await,
        RunStart::Admitted(events) if sse => stream(&state, domain, events).await,
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

/// The end fragments of the stored history, for a visit whose head is still fresh.
fn stored_answer(domain: &str, entries: &[Entry], sse: bool) -> Response {
    if !sse {
        return see_other(domain);
    }
    match stored_parts(domain, entries, false, None, None) {
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

async fn refused(state: &ResultState, domain: &str, refusal: &RunRefusal, sse: bool) -> Response {
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
        )
    };
    response.headers_mut().insert(RETRY_AFTER, retry.into());
    response
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
}

async fn stream(state: &ResultState, domain: String, events: RunEvents) -> Response {
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
        let mut rows = vec![(super::now(), netray_model::Grade::Incomplete)];
        rows.extend(
            self.entries
                .iter()
                .take(SHOWN - 1)
                .map(|e| (e.created_at, e.grade)),
        );
        let live = run
            .sections
            .iter()
            .map(|s| s.protocol.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let sentences = sentences_of(self.entries.iter().take(SHOWN - 1).rev());
        let html = figure_parts(&self.domain, &current, &older, &rows, &live)
            .and_then(|(plate, trace)| spectrum_part(&plate, &trace, &sentences, true));
        self.push("spectrum", html);
    }

    fn start(&mut self) {
        let run = views::live_run(&[]);
        let line = RunLineBody::Line(RunLineView {
            parts: vec!["running".to_string()],
        });
        let html = run_line_part(&line);
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
        let inner = evidence::render_section(&evidence::section(
            &run,
            protocol,
            evidence::live_run_line(),
        ))
        .inspect_err(|e| tracing::warn!(error = %e, "evidence section render failed"))
        .ok()
        .map(EvidenceInner::Section);
        let html = inner.and_then(|inner| evidence_part(protocol, &inner));
        self.push(&format!("evidence-{}", protocol.as_str()), html);
    }

    fn not_stored(&self, done: &Run, recorded: &Recorded) -> Option<Parts> {
        let shown: Shown = not_stored_shown(&self.domain, done, recorded, &self.entries)?;
        assemble(&self.domain, false, shown)
    }

    /// The parts of the history with the recorded entry as its head; when the re-read does not
    /// return it, the finished run stands in for it.
    async fn stored(
        store: &HistoryStore,
        domain: &str,
        done: &Run,
        recorded: &Recorded,
    ) -> Option<Parts> {
        let mut entries = store
            .entries(domain)
            .await
            .inspect_err(|e| tracing::warn!(domain = %domain, error = %e, "history lookup failed"))
            .unwrap_or_default();
        let entry = recorded_entry(domain, done, recorded)?;
        if entries.first().map(|e| &e.id) != Some(&entry.id) {
            entries.insert(0, entry);
        }
        stored_parts(domain, &entries, false, Some(recorded), None)
    }

    async fn finish(&mut self, recorded: Option<Recorded>) {
        let Some(done) = self.done.take() else {
            self.queue.push_back(end());
            return;
        };
        let parts = match recorded {
            None if done.complete => unrecorded_shown(&self.domain, &done, &self.entries)
                .and_then(|shown| assemble(&self.domain, false, shown)),
            None => self.not_stored(&done, &Recorded::NotStored { diff: None }),
            Some(Recorded::NotStored { diff }) => {
                self.not_stored(&done, &Recorded::NotStored { diff })
            }
            Some(stored) => Self::stored(&self.store, &self.domain, &done, &stored).await,
        };
        if let Some(parts) = parts {
            for (id, html) in parts.fragments() {
                self.queue.push_back(fragment(&id, html));
            }
        }
        self.queue.push_back(end());
    }
}
