//! Admission: one run per domain in flight, shared by every caller that asks for it
//! (specs/features/v2-admission/spec.md). A run is a task of its own, so a caller that goes
//! away does not stop it; callers read its sections, its published result and its history
//! record from a [`RunStream`].

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use netray_engine::SectionEvent;
use netray_model::evidence::Block;
use netray_model::run::Run;
use netray_store::Recorded;
use tokio::sync::{Notify, watch};
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

/// Starts or joins the domain's run. `domain` is validated; `prefetch` is not used yet.
/// Must be called inside a tokio runtime.
pub fn admit(
    state: &AppState,
    domain: &str,
    client: IpAddr,
    trigger: Trigger,
    _prefetch: bool,
) -> Admit {
    let key = cache_key(domain);
    let mut runs = state.runs.lock();
    if let Some(rx) = runs.get(&key) {
        return Admit::Admitted(RunStream::new(rx.clone(), true));
    }
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
        run_task(state.clone(), input, trigger, Some(key), tx, live)
            .instrument(tracing::Span::current()),
    );
    Admit::Admitted(RunStream::new(rx, false))
}

/// A run no other caller can join and that is never joined: the DKIM-selector path.
pub(crate) fn start_unregistered(
    state: &AppState,
    input: CheckInput,
    trigger: Trigger,
) -> RunStream {
    let (tx, rx) = watch::channel(RunState::default());
    let live = state.runs.track();
    tokio::spawn(
        run_task(state.clone(), input, trigger, None, tx, live)
            .instrument(tracing::Span::current()),
    );
    RunStream::new(rx, false)
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
) {
    let _deregister = key.map(|key| Deregister {
        runs: Arc::clone(&state.runs),
        key,
        rx: tx.subscribe(),
    });
    let guard = trigger.counted().then(RunGuard::new);
    let started = Instant::now();
    let dkim = input.dkim_selectors.is_some();

    let (output, facts) = match (&state.badge_check_fn, trigger) {
        (Some(f), Trigger::Badge | Trigger::Og) => (f(input.domain.clone()).await, Vec::new()),
        _ => {
            run_check_streaming(&state, input, HARD_DEADLINE, |ev| {
                tx.send_modify(|s| s.sections.push(ev.clone()));
            })
            .await
        }
    };
    if trigger.counted() {
        observe_run(started.elapsed());
    }
    drop(guard);

    let snapshot_id = if output.score.complete {
        crate::routes::create_snapshot(&state, &output.domain, &output).await
    } else {
        None
    };
    let run = crate::history::build_run(&state, &output, &facts);
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
