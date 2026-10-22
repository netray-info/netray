//! Runs with caller-chosen DKIM selectors go through lens's admission as private runs
//! (specs/features/v2-dkim-legacy-urls/spec.md, Phase 2; criteria C4, C17 to C20).
//!
//! `LensRuns::start_with` with a non-empty selector list never joins a run in flight, is
//! throttled per domain like any run, records nothing and starts nothing for a prefetch; the
//! email module receives the list. The registry holds one stub email module that records
//! `ctx.options.dkim_selectors` and can be held open on a gate.

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use crate::common;

use futures::StreamExt;
use lens::api_v1::LensRuns;
use lens::config::Config;
use lens::state::AppState;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, Registry, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRuns, RefusalReason, RunEvent, RunStart, RunTrigger};
use tempfile::NamedTempFile;
use tokio::sync::Notify;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(10);

static CHECKS: LazyLock<Vec<CheckId>> =
    LazyLock::new(|| vec![CheckId::parse("email.email_authentication").unwrap()]);

type Seen = Arc<Mutex<Vec<Option<Vec<String>>>>>;

/// An email module that records the DKIM selectors of every run, optionally waits for `gate`,
/// and answers one passing check.
struct RecordingEmail {
    seen: Seen,
    gate: Option<Arc<Notify>>,
}

impl Module for RecordingEmail {
    fn protocol(&self) -> Protocol {
        Protocol::Email
    }
    fn checks(&self) -> &'static [CheckId] {
        &CHECKS
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn run<'a>(&'a self, ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        self.seen
            .lock()
            .unwrap()
            .push(ctx.options.dkim_selectors.clone());
        let gate = self.gate.clone();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.notified().await;
            }
            SectionOutcome::Measured {
                checks: vec![CheckResult {
                    id: CheckId::parse("email.email_authentication").unwrap(),
                    status: Status::Pass,
                    findings: vec![],
                    evidence: vec![],
                }],
                evidence: vec![],
            }
        })
    }
}

struct Harness {
    runs: LensRuns,
    store: Arc<HistoryStore>,
    seen: Seen,
    gate: Option<Arc<Notify>>,
    _db: NamedTempFile,
}

fn client() -> IpAddr {
    IpAddr::from([127, 0, 0, 1])
}

fn selectors() -> Vec<String> {
    vec!["s1".to_string(), "s2".to_string()]
}

async fn harness(hold: bool) -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.rate_limit.per_ip_per_minute = 1000;
    config.rate_limit.per_ip_burst = 1000;
    config.rate_limit.global_per_minute = 10000;
    config.rate_limit.global_burst = 10000;
    config.admission.client_budget_per_hour = 1000;
    let db = NamedTempFile::new().unwrap();
    config.history.enabled = true;
    config.history.db_path = db.path().to_path_buf();
    let store = HistoryStore::open(db.path()).await.unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);

    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let gate = hold.then(|| Arc::new(Notify::new()));
    let registry = Registry::new()
        .with(Box::new(RecordingEmail {
            seen: seen.clone(),
            gate: gate.clone(),
        }))
        .with_facts(Box::new(common::StubFacts(Facts::default())));
    let mut state = AppState::with_registry(config, registry).unwrap();
    state.history_store = Some(store.clone());
    Harness {
        runs: LensRuns::new(state),
        store,
        seen,
        gate,
        _db: db,
    }
}

/// Every event of an admitted run, in order.
async fn drain(start: RunStart) -> Vec<RunEvent> {
    match start {
        RunStart::Admitted(events) => timeout(WAIT, events.collect::<Vec<_>>())
            .await
            .expect("the run ends"),
        RunStart::Refused(r) => panic!("refused: {:?}", r.reason),
        RunStart::NotStarted => panic!("not started"),
    }
}

fn refused_throttle(start: RunStart) -> bool {
    matches!(start, RunStart::Refused(r) if r.reason == RefusalReason::Throttle)
}

#[tokio::test]
async fn second_selector_start_of_one_domain_is_throttled() {
    let h = harness(false).await;
    let sel = selectors();
    let first = h
        .runs
        .start_with("example.com", client(), RunTrigger::Api, false, &sel)
        .await;
    drain(first).await;
    let second = h
        .runs
        .start_with("example.com", client(), RunTrigger::Api, false, &sel)
        .await;
    assert!(refused_throttle(second), "second selector run: Throttle");
}

#[tokio::test]
async fn selector_start_does_not_join_a_plain_run_in_flight() {
    let h = harness(true).await;
    let plain = h
        .runs
        .start("example.com", client(), RunTrigger::Api, false)
        .await;
    assert!(matches!(plain, RunStart::Admitted(_)), "plain run admitted");
    let with = h
        .runs
        .start_with(
            "example.com",
            client(),
            RunTrigger::Api,
            false,
            &selectors(),
        )
        .await;
    assert!(
        refused_throttle(with),
        "selector start beside a plain run: Throttle"
    );
    h.gate.as_ref().unwrap().notify_one();
    drop(plain);
}

#[tokio::test]
async fn selector_run_records_nothing() {
    let h = harness(false).await;
    let start = h
        .runs
        .start_with(
            "example.com",
            client(),
            RunTrigger::Api,
            false,
            &selectors(),
        )
        .await;
    let events = drain(start).await;
    assert!(
        matches!(events.last(), Some(RunEvent::Recorded(None))),
        "the stream ends with Recorded(None)"
    );
    assert!(events.iter().any(|e| matches!(e, RunEvent::Done(_))));
    let entries = h.store.entries("example.com").await.unwrap();
    assert!(
        entries.is_empty(),
        "history holds no entry: {}",
        entries.len()
    );
}

#[tokio::test]
async fn selector_prefetch_starts_nothing() {
    let h = harness(false).await;
    let start = h
        .runs
        .start_with("example.com", client(), RunTrigger::Api, true, &selectors())
        .await;
    assert!(matches!(start, RunStart::NotStarted));
    assert!(h.seen.lock().unwrap().is_empty(), "no module ran");
}

#[tokio::test]
async fn email_module_receives_the_selectors() {
    let h = harness(false).await;
    let start = h
        .runs
        .start_with(
            "example.com",
            client(),
            RunTrigger::Api,
            false,
            &selectors(),
        )
        .await;
    drain(start).await;
    let seen = h.seen.lock().unwrap().clone();
    assert_eq!(seen, vec![Some(vec!["s1".to_string(), "s2".to_string()])]);
}
