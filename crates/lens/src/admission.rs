//! Admission: one run per domain in flight, shared by every caller that asks for it
//! (specs/features/v2-admission/spec.md). A run is a task of its own, so a caller that goes
//! away does not stop it; callers read its sections, its published result and its history
//! record from a [`RunStream`].

use std::collections::HashMap;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use governor::clock::{Clock, DefaultClock};
use netray_engine::SectionEvent;
use netray_model::evidence::Block;
use netray_model::run::Run;
use netray_store::Recorded;
use tokio::sync::{Notify, OwnedSemaphorePermit, watch};
use tracing::Instrument;

use crate::cache::cache_key;
use crate::check::{CheckInput, CheckOutput, HARD_DEADLINE, run_check_streaming};
use crate::metrics::{RunGuard, observe_run};
use crate::state::AppState;

/// What asked for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Visit,
    Recheck,
    Api,
    Badge,
    Og,
}

impl Trigger {
    fn label(self) -> &'static str {
        match self {
            Trigger::Visit => "visit",
            Trigger::Recheck => "recheck",
            Trigger::Api => "api",
            Trigger::Badge => "badge",
            Trigger::Og => "og",
        }
    }

    fn counted(self) -> bool {
        matches!(self, Trigger::Visit | Trigger::Recheck | Trigger::Api)
    }
}

/// What a finished run published.
pub struct Published {
    pub output: CheckOutput,
    pub facts: Vec<Block>,
    pub snapshot_id: Option<String>,
    pub run: Run,
}

pub enum RunEvent {
    Section(SectionEvent),
    Done(Arc<Published>),
    Recorded(Option<Recorded>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Throttle,
    Budget,
    IpLimit,
    GlobalLimit,
    Slots,
}

/// Why a request got no run, and when to ask again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub reason: Reason,
    pub retry_after_s: u64,
    pub last_run_ago_s: Option<u64>,
    pub last_incomplete: bool,
}

pub enum Admit {
    Admitted(RunStream),
    Refused(Refusal),
    NotStarted,
}

#[derive(Default)]
struct RunState {
    sections: Vec<SectionEvent>,
    done: Option<Arc<Published>>,
    recorded: Option<Option<Recorded>>,
}

/// The runs in flight, by cache key.
#[derive(Default)]
pub struct Runs {
    map: Mutex<HashMap<String, watch::Receiver<RunState>>>,
    live: AtomicUsize,
    idle: Notify,
}

impl Runs {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, watch::Receiver<RunState>>> {
        self.map.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn track(self: &Arc<Self>) -> LiveRun {
        self.live.fetch_add(1, Ordering::SeqCst);
        LiveRun(Arc::clone(self))
    }

    /// Waits until no run task is alive, registered or not.
    pub async fn drain(&self) {
        loop {
            let notified = self.idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.live.load(Ordering::SeqCst) == 0 {
                return;
            }
            notified.await;
        }
    }
}

/// Counts one live run task; dropping it, panic included, lets `drain` re-check.
struct LiveRun(Arc<Runs>);

impl Drop for LiveRun {
    fn drop(&mut self) {
        self.0.live.fetch_sub(1, Ordering::SeqCst);
        self.0.idle.notify_waiters();
    }
}

/// A caller's view of one run: its sections, then `Done`, then `Recorded`, then nothing.
pub struct RunStream {
    rx: watch::Receiver<RunState>,
    cursor: usize,
    done_sent: bool,
    recorded_sent: bool,
    joined: bool,
}

impl RunStream {
    fn new(rx: watch::Receiver<RunState>, joined: bool) -> Self {
        Self {
            rx,
            cursor: 0,
            done_sent: false,
            recorded_sent: false,
            joined,
        }
    }

    /// True when this stream joined a run another request started.
    pub fn joined(&self) -> bool {
        self.joined
    }

    /// The next event; `None` after `Recorded`, or when the run's task ended without one.
    pub async fn next(&mut self) -> Option<RunEvent> {
        let mut closed = false;
        loop {
            if self.recorded_sent {
                return None;
            }
            if let Some(event) = self.poll_state() {
                return Some(event);
            }
            if closed {
                return None;
            }
            closed = self.rx.changed().await.is_err();
        }
    }

    fn poll_state(&mut self) -> Option<RunEvent> {
        let state = self.rx.borrow();
        if let Some(ev) = state.sections.get(self.cursor) {
            self.cursor += 1;
            return Some(RunEvent::Section(ev.clone()));
        }
        if !self.done_sent {
            let done = state.done.as_ref()?;
            self.done_sent = true;
            return Some(RunEvent::Done(Arc::clone(done)));
        }
        let recorded = state.recorded.as_ref()?;
        self.recorded_sent = true;
        Some(RunEvent::Recorded(recorded.clone()))
    }
}

/// One run per domain may start in this window, counted from the run's start.
pub const THROTTLE: Duration = Duration::from_secs(60);

struct Record {
    started: tokio::time::Instant,
    complete: Option<bool>,
}

/// When each domain's last run started, and whether it ended complete.
#[derive(Default)]
pub struct Throttle(Mutex<HashMap<String, Record>>);

impl Throttle {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Record>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn finish(&self, key: &str, started: tokio::time::Instant, complete: bool) {
        if let Some(record) = self.lock().get_mut(key)
            && record.started == started
        {
            record.complete = Some(complete);
        }
    }
}

/// What a started run holds: its throttle record and one slot.
struct Ticket {
    key: String,
    started: tokio::time::Instant,
    _permit: OwnedSemaphorePermit,
}

/// A refusal that counts under `netray_admission_rejections_total`, except the limiters', which the
/// route counts (a limit check called twice for one request must not count twice).
fn refused(
    reason: Reason,
    retry_after_s: u64,
    last_run_ago_s: Option<u64>,
    last_incomplete: bool,
) -> Refusal {
    let label = match reason {
        Reason::Throttle => Some("throttle"),
        Reason::Budget => Some("budget"),
        Reason::IpLimit | Reason::GlobalLimit => None,
        Reason::Slots => Some("slots"),
    };
    if let Some(label) = label {
        metrics::counter!("netray_admission_rejections_total", "reason" => label).increment(1);
    }
    Refusal {
        reason,
        retry_after_s,
        last_run_ago_s,
        last_incomplete,
    }
}

/// The per-IP and global limiters. The caller that refuses on the result counts
/// `netray_admission_rejections_total{reason="limit"}`.
pub fn check_limits(state: &AppState, client: IpAddr) -> Result<(), Refusal> {
    crate::security::check_rate_limit(&state.per_ip_limiter, &state.global_limiter, client).map_err(
        |r| {
            let reason = if r.scope == "global" {
                Reason::GlobalLimit
            } else {
                Reason::IpLimit
            };
            refused(reason, r.retry_after_secs, None, false)
        },
    )
}

/// Throttle, then slot, then the client's budget (when `budgeted`) or the domain's badge
/// recompute limiter (when `recompute` names a scope); a refusal spends nothing.
fn gate(
    state: &AppState,
    key: String,
    client: IpAddr,
    budgeted: bool,
    recompute: Option<&'static str>,
) -> Result<Ticket, Refusal> {
    let mut throttle = state.throttle.lock();
    let now = tokio::time::Instant::now();
    throttle.retain(|_, r| now.saturating_duration_since(r.started) < THROTTLE);
    if let Some(record) = throttle.get(&key) {
        let elapsed = now.saturating_duration_since(record.started);
        let left = THROTTLE.saturating_sub(elapsed);
        let retry = (left.as_secs() + u64::from(left.subsec_nanos() > 0)).max(1);
        return Err(refused(
            Reason::Throttle,
            retry,
            Some(elapsed.as_secs()),
            record.complete == Some(false),
        ));
    }
    let Ok(permit) = state.slots.clone().try_acquire_owned() else {
        return Err(refused(Reason::Slots, 1, None, false));
    };
    if budgeted {
        let Some(limiter) = &state.client_budget else {
            return Err(refused(Reason::Budget, 3600, None, false));
        };
        if let Err(not_until) = limiter.check_key(&client) {
            let wait = not_until.wait_time_from(DefaultClock::default().now());
            return Err(refused(Reason::Budget, wait.as_secs().max(1), None, false));
        }
    }
    if let Some(scope) = recompute
        && let Err(r) = netray_common::rate_limit::check_keyed_cost(
            &state.badge_recompute_limiter,
            &key,
            NonZeroU32::new(1).unwrap(),
            scope,
            "netray",
        )
    {
        return Err(refused(Reason::Throttle, r.retry_after_secs, None, false));
    }
    throttle.insert(
        key.clone(),
        Record {
            started: now,
            complete: None,
        },
    );
    Ok(Ticket {
        key,
        started: now,
        _permit: permit,
    })
}

/// Starts or joins the domain's run. `domain` is validated; a `prefetch` request that finds no run in
/// flight gets `NotStarted`, with no throttle record and no budget token.
/// Order: join, prefetch, throttle, slot, then the budget (visit, recheck, api) or
/// the recompute limiter (badge, og), then start. Must be called inside a tokio runtime.
pub fn admit(
    state: &AppState,
    domain: &str,
    client: IpAddr,
    trigger: Trigger,
    prefetch: bool,
) -> Admit {
    let key = cache_key(domain);
    let mut runs = state.runs.lock();
    if let Some(rx) = runs.get(&key) {
        return Admit::Admitted(RunStream::new(rx.clone(), true));
    }
    if prefetch {
        return Admit::NotStarted;
    }
    let recompute = match trigger {
        Trigger::Badge => Some("badge"),
        Trigger::Og => Some("og"),
        _ => None,
    };
    let ticket = match gate(state, key.clone(), client, trigger.counted(), recompute) {
        Ok(ticket) => ticket,
        Err(r) => return Admit::Refused(r),
    };
    let (tx, rx) = watch::channel(RunState::default());
    runs.insert(key.clone(), rx.clone());
    drop(runs);
    let input = CheckInput {
        domain: domain.to_string(),
        dkim_selectors: None,
        client_ip: Some(client),
        request_id: None,
    };
    let live = state.runs.track();
    tokio::spawn(
        run_task(state.clone(), input, trigger, Some(key), tx, live, ticket)
            .instrument(tracing::Span::current()),
    );
    Admit::Admitted(RunStream::new(rx, false))
}

/// A run no other caller can join and that is never joined: the DKIM-selector path. It is
/// bound by the throttle, the slots and the budget like any fresh run.
pub(crate) fn admit_dkim(state: &AppState, input: CheckInput) -> Result<RunStream, Refusal> {
    let client = input
        .client_ip
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    let ticket = gate(state, cache_key(&input.domain), client, true, None)?;
    let (tx, rx) = watch::channel(RunState::default());
    let live = state.runs.track();
    tokio::spawn(
        run_task(state.clone(), input, Trigger::Api, None, tx, live, ticket)
            .instrument(tracing::Span::current()),
    );
    Ok(RunStream::new(rx, false))
}

/// Reads `stream` to its `Done`.
pub(crate) async fn until_done(stream: &mut RunStream) -> Option<Arc<Published>> {
    while let Some(event) = stream.next().await {
        if let RunEvent::Done(published) = event {
            return Some(published);
        }
    }
    None
}

/// Takes the run out of the map when the task ends, panic included, unless a newer run
/// already took the key.
struct Deregister {
    runs: Arc<Runs>,
    key: String,
    rx: watch::Receiver<RunState>,
}

impl Drop for Deregister {
    fn drop(&mut self) {
        let mut runs = self.runs.lock();
        if runs
            .get(&self.key)
            .is_some_and(|r| r.same_channel(&self.rx))
        {
            runs.remove(&self.key);
        }
    }
}

async fn run_task(
    state: AppState,
    input: CheckInput,
    trigger: Trigger,
    key: Option<String>,
    tx: watch::Sender<RunState>,
    _live: LiveRun,
    ticket: Ticket,
) {
    let _deregister = key.map(|key| Deregister {
        runs: Arc::clone(&state.runs),
        key,
        rx: tx.subscribe(),
    });
    let guard = trigger.counted().then(RunGuard::new);
    let started = Instant::now();
    let dkim = input.dkim_selectors.is_some();

    let mut first_section = true;
    let (output, facts) = match (&state.badge_check_fn, trigger) {
        (Some(f), Trigger::Badge | Trigger::Og) => (f(input.domain.clone()).await, Vec::new()),
        _ => {
            run_check_streaming(&state, input, HARD_DEADLINE, |ev| {
                if std::mem::take(&mut first_section) {
                    metrics::histogram!("netray_time_to_first_section_seconds")
                        .record(started.elapsed().as_secs_f64());
                }
                tx.send_modify(|s| s.sections.push(ev.clone()));
            })
            .await
        }
    };
    metrics::histogram!("netray_time_to_result_seconds").record(started.elapsed().as_secs_f64());
    if trigger.counted() {
        observe_run(started.elapsed());
    }
    drop(guard);
    state
        .throttle
        .finish(&ticket.key, ticket.started, output.score.complete);

    let snapshot_id = if output.score.complete {
        crate::routes::create_snapshot(&state, &output.domain, &output).await
    } else {
        None
    };
    let run = crate::history::build_run(&state, &output, &facts);
    crate::metrics::count_run(trigger.label(), &run, &state.scoring_profile);
    let domain = output.domain.clone();
    let published = Arc::new(Published {
        output,
        facts,
        snapshot_id,
        run,
    });
    tx.send_modify(|s| s.done = Some(Arc::clone(&published)));

    let recorded = crate::history::record(&state, &domain, dkim, &published.run).await;
    crate::cache::hold(&state, recorded.as_ref(), &published).await;
    tx.send_modify(|s| s.recorded = Some(recorded));
    drop(ticket);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn drain_waits_for_live_runs() {
        let runs = Arc::new(Runs::default());
        tokio::time::timeout(Duration::from_millis(10), runs.drain())
            .await
            .expect("drain returns at once with no runs");

        let live = runs.track();
        let held = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            drop(live);
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(10), runs.drain())
                .await
                .is_err(),
            "drain must wait while a run is alive"
        );
        tokio::time::timeout(Duration::from_secs(5), runs.drain())
            .await
            .expect("drain completes once the run ends");
        held.await.unwrap();
    }
}
