//! Admission: one run per domain in flight, shared by every caller that asks for it
//! (specs/features/v2-admission/spec.md). A run is a task of its own, so a caller that goes
//! away does not stop it; callers read its sections, its published result and its history
//! record from a [`RunStream`]. Copied from lens's `admission.rs` (spec v2-cutover, Phase 6)
//! without the V1 parts: joining a recording run, the snapshot and the V1 cache.

mod head;
mod image_budget;
mod rate_limit;
mod runs;
mod state;

pub use head::{HeadLookup, cache_key, head};
pub use image_budget::ImageBudget;
pub use rate_limit::{GlobalRateLimiter, PerIpRateLimiter, check_rate_limit};
pub use runs::AdmittedRuns;
pub use state::{
    ApexState, BadgeCheckFn, BadgeRecomputeLimiter, ClientBudgetLimiter, load_scoring_profile,
};

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

use netray_engine::domain_run::{CheckInput, CheckOutput, HARD_DEADLINE, build_run, run_domain};

use crate::metrics::{RunGuard, observe_run};

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
    /// The site-wide budget of badge and OG runs is spent (`[admission] image_runs_per_minute`).
    ImageBudget,
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

/// A refusal, not yet counted: the caller counts it with [`count_rejection`], or answers it
/// another way.
fn refused(
    reason: Reason,
    retry_after_s: u64,
    last_run_ago_s: Option<u64>,
    last_incomplete: bool,
) -> Refusal {
    Refusal {
        reason,
        retry_after_s,
        last_run_ago_s,
        last_incomplete,
    }
}

/// Counts a refusal in `netray_admission_rejections_total`.
pub fn count_rejection(refusal: &Refusal) {
    let label = match refusal.reason {
        Reason::Throttle => "throttle",
        Reason::Budget => "budget",
        Reason::IpLimit | Reason::GlobalLimit => "limit",
        Reason::Slots => "slots",
        Reason::ImageBudget => "image_budget",
    };
    metrics::counter!("netray_admission_rejections_total", "reason" => label).increment(1);
}

/// The per-IP and global limiters. The caller that refuses on the result counts it with
/// [`count_rejection`] (a limit check called twice for one request must not count twice).
pub fn check_limits(state: &ApexState, client: IpAddr) -> Result<(), Refusal> {
    check_rate_limit(&state.per_ip_limiter, &state.global_limiter, client).map_err(|r| {
        let reason = if r.scope == "global" {
            Reason::GlobalLimit
        } else {
            Reason::IpLimit
        };
        refused(reason, r.retry_after_secs, None, false)
    })
}

/// Throttle, then slot, then the client's budget (when `budgeted`) or, when `recompute` names a
/// scope, the site-wide image budget (peeked), the domain's badge recompute limiter and then the
/// image budget's token; a refusal spends nothing.
fn gate(
    state: &ApexState,
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
    if let Some(scope) = recompute {
        if let Err(retry) = state.image_budget.peek() {
            return Err(refused(Reason::ImageBudget, retry, None, false));
        }
        if let Err(r) = netray_common::rate_limit::check_keyed_cost(
            &state.badge_recompute_limiter,
            &key,
            NonZeroU32::new(1).unwrap(),
            scope,
            "netray",
        ) {
            return Err(refused(Reason::Throttle, r.retry_after_secs, None, false));
        }
        state.image_budget.take();
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
/// the image budget and the recompute limiter (badge, og), then start. Must be called inside a tokio runtime.
pub fn admit(
    state: &ApexState,
    domain: &str,
    client: IpAddr,
    trigger: Trigger,
    prefetch: bool,
) -> Admit {
    let admitted = admit_uncounted(state, domain, client, trigger, prefetch);
    if let Admit::Refused(r) = &admitted {
        count_rejection(r);
    }
    admitted
}

/// `admit` without counting a refusal: the caller counts it, or answers it another way.
pub fn admit_uncounted(
    state: &ApexState,
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
    let live = state.runs.track();
    let input = CheckInput {
        domain: domain.to_string(),
        dkim_selectors: None,
    };
    tokio::spawn(
        run_task(state.clone(), input, trigger, Some(key), tx, live, ticket)
            .instrument(tracing::Span::current()),
    );
    Admit::Admitted(RunStream::new(rx, false))
}

/// A run with caller-supplied DKIM selectors: never joins and is never joined, and is not
/// recorded (lens's `admit_dkim`). It is bound by the throttle, the slots and the budget like
/// any fresh run, and counts as a visit.
pub fn admit_dkim(
    state: &ApexState,
    domain: &str,
    client: IpAddr,
    dkim_selectors: Vec<String>,
) -> Result<RunStream, Refusal> {
    let ticket = gate(state, cache_key(domain), client, true, None).inspect_err(count_rejection)?;
    let (tx, rx) = watch::channel(RunState::default());
    let live = state.runs.track();
    let input = CheckInput {
        domain: domain.to_string(),
        dkim_selectors: Some(dkim_selectors),
    };
    tokio::spawn(
        run_task(state.clone(), input, Trigger::Visit, None, tx, live, ticket)
            .instrument(tracing::Span::current()),
    );
    Ok(RunStream::new(rx, false))
}

/// Reads `stream` to its `Done`.
pub async fn until_done(stream: &mut RunStream) -> Option<Arc<Published>> {
    while let Some(event) = stream.next().await {
        if let RunEvent::Done(published) = event {
            return Some(published);
        }
    }
    None
}

/// Takes the run out of the map before it publishes `Recorded`, or when the task ends
/// without it, panic included, unless a newer run already took the key.
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
    state: ApexState,
    input: CheckInput,
    trigger: Trigger,
    key: Option<String>,
    tx: watch::Sender<RunState>,
    _live: LiveRun,
    ticket: Ticket,
) {
    let deregister = key.map(|key| Deregister {
        runs: Arc::clone(&state.runs),
        key,
        rx: tx.subscribe(),
    });
    let dkim = input.dkim_selectors.is_some();
    let guard = trigger.counted().then(RunGuard::new);
    let started = Instant::now();

    let mut first_section = true;
    let (output, facts, run) = match (&state.badge_check_fn, trigger) {
        (Some(f), Trigger::Badge | Trigger::Og) => {
            let output = f(input.domain).await;
            let run = build_run(&state.scoring_profile, &output, &[]);
            (output, Vec::new(), run)
        }
        _ => {
            let done = run_domain(
                &state.registry,
                &state.plan,
                state.resolve_budget,
                &state.scoring_profile,
                input,
                HARD_DEADLINE,
                |ev| {
                    if std::mem::take(&mut first_section) {
                        metrics::histogram!("netray_time_to_first_section_seconds")
                            .record(started.elapsed().as_secs_f64());
                    }
                    tx.send_modify(|s| s.sections.push(ev.clone()));
                },
            )
            .await;
            (done.output, done.facts, done.run)
        }
    };
    let elapsed = started.elapsed();
    metrics::histogram!("netray_time_to_result_seconds").record(elapsed.as_secs_f64());
    if trigger.counted() {
        observe_run(elapsed);
    }
    drop(guard);
    state
        .throttle
        .finish(&ticket.key, ticket.started, output.score.complete);

    crate::metrics::count_run(trigger.label(), &run, &state.scoring_profile);
    let domain = output.domain.clone();
    let published = Arc::new(Published { output, facts, run });
    tx.send_modify(|s| s.done = Some(Arc::clone(&published)));

    let recorded = match &state.history_store {
        Some(store) => {
            let volatile = netray_engine::domain_run::volatile(&state.registry);
            let profile = &state.scoring_profile;
            netray_store::record_timed(
                store,
                &domain,
                dkim,
                &published.run,
                &volatile,
                u64::try_from(elapsed.as_millis()).ok(),
                |previous, current| {
                    netray_engine::history::diff(previous, current, &volatile, profile)
                },
            )
            .await
        }
        None => None,
    };
    // Out of the register before the stream ends: a caller that read `Recorded` never joins this run.
    drop(deregister);
    tx.send_modify(|s| s.recorded = Some(recorded));
    drop(ticket);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use netray_engine::domain_run::OverallScore;
    use netray_engine::profile::Profiles;

    use super::*;
    use crate::config::ApexConfig;

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

    /// A run whose stream has ended on `Recorded` is no longer in the register, so a prefetch
    /// that comes after it gets `NotStarted` instead of joining the finished run. The test holds
    /// the register's lock while it reads the run: the run's task cannot deregister meanwhile, so
    /// the order of deregistration and `Recorded` is observed, not raced.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    // Holding the register's std mutex across the reads is the point: it blocks deregistration.
    #[allow(clippy::await_holding_lock)]
    async fn recorded_follows_deregistration() {
        let mut config = ApexConfig::default();
        config.limits.rate_limit_per_ip_per_minute = 1000;
        config.limits.rate_limit_per_ip_burst = 1000;
        let mut state = ApexState::new(
            config,
            netray_engine::Registry::new(),
            Vec::new(),
            Duration::from_millis(1500),
            Profiles::current().clone(),
        );
        state.badge_check_fn = Some(Arc::new(|domain: String| {
            Box::pin(async move {
                CheckOutput {
                    domain,
                    sections: HashMap::new(),
                    score: OverallScore {
                        sections: HashMap::new(),
                        overall_percentage: 80.0,
                        grade: "B".to_string(),
                        hard_fail_triggered: false,
                        hard_fail_checks: vec![],
                        not_applicable: HashMap::new(),
                        complete: true,
                    },
                    duration_ms: 1,
                }
            })
        }));
        let client = IpAddr::from([127, 0, 0, 1]);
        let Admit::Admitted(mut stream) =
            admit(&state, "example.com", client, Trigger::Badge, false)
        else {
            panic!("the first admit starts a run");
        };

        let register = state.runs.lock();
        let recorded = tokio::time::timeout(Duration::from_millis(500), async {
            while let Some(event) = stream.next().await {
                if matches!(event, RunEvent::Recorded(_)) {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false);
        let registered = register.contains_key(&cache_key("example.com"));
        drop(register);
        assert!(
            !(recorded && registered),
            "the stream ended on Recorded while the run was still joinable"
        );

        while stream.next().await.is_some() {}
        assert!(
            matches!(
                admit(&state, "example.com", client, Trigger::Badge, true),
                Admit::NotStarted
            ),
            "a prefetch after the run's end: NotStarted"
        );
    }
}
