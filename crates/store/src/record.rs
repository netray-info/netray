//! Recording a domain run into the history, with its metrics, and the hourly retention sweep.
//! Copied from lens's `history.rs` (spec v2-cutover, Phase 6); the diff and the volatile paths
//! come from the caller, so the store keeps depending on `netray-model` alone among the
//! workspace crates.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use netray_model::diff::{ChangeClass, Diff};
use netray_model::run::Run;

use crate::{Head, HistoryStore, Recorded};

const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);

/// Records (complete) or diffs (incomplete) `run`; a failure is logged and counted, never
/// returned. Runs with caller-supplied DKIM selectors are skipped. `diff` computes the diff of
/// (predecessor, current) as [`HistoryStore::record`] takes it.
pub async fn record<F>(
    store: &HistoryStore,
    domain: &str,
    dkim_selectors: bool,
    run: &Run,
    volatile: &[&str],
    diff: F,
) -> Option<Recorded>
where
    F: FnOnce(&Run, &Run) -> Diff,
{
    if dkim_selectors {
        return None;
    }
    let previous = store.head(domain).await.ok().flatten();
    match store.record(domain, run, volatile, now_unix(), diff).await {
        Ok(recorded) => {
            observe_recorded(previous.as_ref(), &recorded);
            Some(recorded)
        }
        Err(e) => {
            tracing::error!(error = %e, domain = %domain, "history record failed");
            metrics::counter!("netray_history_record_errors_total").increment(1);
            None
        }
    }
}

/// Counts what a recorded run says: that it had a predecessor, how long ago that ran, and the
/// checks the run fixed. An incomplete run stores nothing and counts nothing here.
fn observe_recorded(previous: Option<&Head>, recorded: &Recorded) {
    if matches!(recorded, Recorded::NotStored { .. }) {
        return;
    }
    if let Some(previous) = previous {
        metrics::counter!("netray_runs_with_predecessor_total").increment(1);
        let interval = now_unix().saturating_sub(previous.last_seen_at).max(0);
        metrics::histogram!("netray_run_interval_seconds").record(interval as f64);
    }
    if let Recorded::Changed { diff, .. } = recorded {
        for change in diff.checks.iter().filter(|c| c.class == ChangeClass::Fixed) {
            metrics::counter!("netray_fixes_observed_total", "section" => change.id.protocol().as_str())
                .increment(1);
        }
    }
}

/// Sets `netray_history_entries` and `netray_history_domains` from the store.
pub async fn set_gauges(store: &HistoryStore) {
    match store.totals().await {
        Ok((entries, domains)) => {
            metrics::gauge!("netray_history_entries").set(entries as f64);
            metrics::gauge!("netray_history_domains").set(domains as f64);
        }
        Err(e) => tracing::warn!("history totals error: {e}"),
    }
}

/// The one Unix-seconds clock of the history.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The hourly retention sweep of the history store.
pub async fn run_sweep_loop(store: Arc<HistoryStore>) {
    loop {
        tokio::time::sleep(SWEEP_INTERVAL).await;
        match store.sweep(now_unix()).await {
            Ok(n) => tracing::info!("history sweep deleted {n} entries"),
            Err(e) => tracing::warn!("history sweep error: {e}"),
        }
        set_gauges(&store).await;
    }
}
