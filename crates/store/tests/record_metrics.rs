//! `netray_store::record` and the history gauges count what lens's `history.rs` counted (spec
//! v2-cutover Phase 6; the store halves of lens's `tests/netray_metrics.rs` C9 and C12 and of
//! `tests/history_record.rs` C10/C11, on the store alone).
//!
//! metrics_util 0.20's `Snapshotter::snapshot()` swaps every value to 0 on read: the accumulator
//! reads once per call and sums across reads.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_model::diff::{Diff, diff};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded, now_unix, record, run_sweep_loop, set_gauges};

const DOMAIN: &str = "example.com";

#[derive(Default, Clone)]
struct Total {
    counter: u64,
    gauge: Option<f64>,
    observations: usize,
}

struct Acc {
    snap: Snapshotter,
    total: RefCell<HashMap<String, Total>>,
}

impl Acc {
    fn new(snap: Snapshotter) -> Self {
        Self {
            snap,
            total: RefCell::new(HashMap::new()),
        }
    }

    /// The series `name` with its labels as `name{k=v,...}`.
    fn read(&self, id: &str) -> Total {
        let mut total = self.total.borrow_mut();
        for (k, _, _, v) in self.snap.snapshot().into_vec() {
            let labels: Vec<String> = k
                .key()
                .labels()
                .map(|l| format!("{}={}", l.key(), l.value()))
                .collect();
            let key = if labels.is_empty() {
                k.key().name().to_string()
            } else {
                format!("{}{{{}}}", k.key().name(), labels.join(","))
            };
            let t = total.entry(key).or_default();
            match v {
                DebugValue::Counter(n) => t.counter += n,
                DebugValue::Gauge(g) => {
                    if g.into_inner() != 0.0 {
                        t.gauge = Some(g.into_inner());
                    }
                }
                DebugValue::Histogram(h) => t.observations += h.len(),
            }
        }
        total.get(id).cloned().unwrap_or_default()
    }
}

fn run(status: Status, complete: bool) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

fn d(p: &Run, c: &Run) -> Diff {
    diff(p, c, &[], None)
}

async fn fresh() -> (Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (Arc::new(store), dir)
}

// lens netray_metrics C9: a run with a predecessor and one fixed tls transition counts the
// predecessor, the fix and the interval; the first run counts none of them.
#[tokio::test(flavor = "current_thread")]
async fn a_recorded_run_with_a_predecessor_counts_predecessor_fix_and_interval() {
    let recorder = DebuggingRecorder::new();
    let acc = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (store, _dir) = fresh().await;

    let first = record(&store, DOMAIN, false, &run(Status::Fail, true), &[], d).await;
    assert!(matches!(first, Some(Recorded::First { .. })), "{first:?}");
    assert_eq!(acc.read("netray_runs_with_predecessor_total").counter, 0);

    let second = record(&store, DOMAIN, false, &run(Status::Pass, true), &[], d).await;
    assert!(
        matches!(second, Some(Recorded::Changed { .. })),
        "{second:?}"
    );
    assert_eq!(acc.read("netray_runs_with_predecessor_total").counter, 1);
    assert_eq!(
        acc.read("netray_fixes_observed_total{section=tls}").counter,
        1
    );
    assert_eq!(acc.read("netray_run_interval_seconds").observations, 1);
}

// lens history_record C11: an incomplete run stores nothing and counts nothing.
#[tokio::test(flavor = "current_thread")]
async fn an_incomplete_run_is_not_stored_and_counts_nothing() {
    let recorder = DebuggingRecorder::new();
    let acc = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (store, _dir) = fresh().await;

    record(&store, DOMAIN, false, &run(Status::Fail, true), &[], d).await;
    let got = record(&store, DOMAIN, false, &run(Status::Pass, false), &[], d).await;
    assert!(matches!(got, Some(Recorded::NotStored { .. })), "{got:?}");
    assert_eq!(store.entries(DOMAIN).await.unwrap().len(), 1);
    assert_eq!(acc.read("netray_runs_with_predecessor_total").counter, 0);
    assert_eq!(acc.read("netray_run_interval_seconds").observations, 0);
}

// lens history_record C10: a run with caller-supplied DKIM selectors records nothing.
#[tokio::test]
async fn dkim_selectors_record_nothing() {
    let (store, _dir) = fresh().await;
    let got = record(&store, DOMAIN, true, &run(Status::Pass, true), &[], d).await;
    assert!(got.is_none());
    assert!(store.head(DOMAIN).await.unwrap().is_none());
}

// lens netray_metrics C12: the history gauges are set from the store and again after a sweep.
#[tokio::test(flavor = "current_thread")]
async fn history_gauges_are_set_and_set_again_after_a_sweep() {
    let recorder = DebuggingRecorder::new();
    let acc = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (store, _dir) = fresh().await;
    let now = now_unix();
    // One domain unseen for 100 days (the sweep drops it), one seen now.
    for (domain, seen) in [
        ("old.example.com", now - 100 * 86_400),
        ("new.example.com", now),
    ] {
        store
            .record(domain, &run(Status::Pass, true), &[], seen, d)
            .await
            .unwrap();
    }

    set_gauges(&store).await;
    assert_eq!(acc.read("netray_history_entries").gauge, Some(2.0));
    assert_eq!(acc.read("netray_history_domains").gauge, Some(2.0));

    tokio::spawn(run_sweep_loop(Arc::clone(&store)));
    // The loop starts its hourly sleep before the clock moves.
    tokio::task::yield_now().await;
    // The hourly sweep fires on tokio's clock.
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(3601)).await;
    tokio::time::resume();
    let mut domains = acc.read("netray_history_domains").gauge;
    for _ in 0..250 {
        if domains == Some(1.0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        domains = acc.read("netray_history_domains").gauge;
    }
    assert_eq!(domains, Some(1.0), "the sweep dropped one domain");
    assert_eq!(acc.read("netray_history_entries").gauge, Some(1.0));
}
