//! Recording lens's runs into the V2 history (`netray-store`).

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use netray_model::evidence::{Block, EVIDENCE_SCHEMA_VERSION};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;

use crate::check::{CheckOutput, SectionError};
use crate::state::AppState;

const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);

const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

fn protocol_for(section: &str) -> Option<Protocol> {
    PROTOCOLS.into_iter().find(|p| p.as_str() == section)
}

/// Every module's volatile paths (state.registry, each protocol).
pub fn volatile(state: &AppState) -> Vec<&'static str> {
    PROTOCOLS
        .into_iter()
        .filter_map(|p| state.registry.module(p))
        .flat_map(|m| m.volatile().iter().map(|path| path.0))
        .collect()
}

fn catalogue_checks(protocol: Protocol, status: Status) -> impl Iterator<Item = RunCheck> {
    netray_engine::catalogue()
        .entries()
        .iter()
        .filter(move |e| e.protocol == protocol)
        .map(move |e| RunCheck {
            id: e.id.clone(),
            status,
            findings: vec![],
            evidence: vec![],
        })
}

/// The Run lens scored: one section per scored section, its checks, every answering section's
/// evidence plus `facts`, and the grade and completeness of `output.score`.
pub fn build_run(state: &AppState, output: &CheckOutput, facts: &[Block]) -> Run {
    let mut names: Vec<&String> = output.sections.keys().collect();
    names.sort();

    let mut sections = Vec::new();
    let mut checks = Vec::new();
    let mut blocks = Vec::new();
    for name in names {
        let Some(protocol) = protocol_for(name) else {
            continue;
        };
        let (outcome, reason) = match &output.sections[name] {
            Ok(result) => {
                checks.extend(result.checks.iter().filter_map(|c| {
                    Some(RunCheck {
                        id: CheckId::parse(&format!("{name}.{}", c.name)).ok()?,
                        status: c.verdict.clone().into(),
                        findings: c.messages.clone(),
                        evidence: c.evidence.clone(),
                    })
                }));
                blocks.extend(result.evidence.iter().cloned());
                (SectionKind::Measured, None)
            }
            Err(SectionError::NotApplicable { reason }) => {
                checks.extend(catalogue_checks(protocol, Status::NotApplicable));
                (SectionKind::NotApplicable, Some(reason.clone()))
            }
            Err(e) => {
                checks.extend(catalogue_checks(protocol, Status::Unmeasured));
                (SectionKind::Incomplete, Some(format!("{e:?}")))
            }
        };
        sections.push(RunSection {
            protocol,
            outcome,
            reason,
        });
    }
    blocks.extend(facts.iter().cloned());

    Run {
        profile_version: state.scoring_profile.version,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: crate::score::parse_grade(&output.score.grade).unwrap_or(Grade::Incomplete),
        complete: output.score.complete,
        sections,
        checks,
        blocks,
    }
}

/// Records (complete) or diffs (incomplete) `run`; a failure is logged and counted, never
/// returned. Runs with caller-supplied DKIM selectors are skipped.
pub(crate) async fn record(
    state: &AppState,
    domain: &str,
    dkim_selectors: bool,
    run: &Run,
) -> Option<netray_store::Recorded> {
    let store = state.history_store.as_ref()?;
    if dkim_selectors {
        return None;
    }
    let volatile = volatile(state);
    let profile = &state.scoring_profile;
    match store
        .record(domain, run, &volatile, now_unix(), |prev, cur| {
            netray_engine::history::diff(prev, cur, &volatile, profile)
        })
        .await
    {
        Ok(recorded) => Some(recorded),
        Err(e) => {
            tracing::error!(error = %e, domain = %domain, "history record failed");
            metrics::counter!("lens_history_record_errors_total").increment(1);
            None
        }
    }
}

/// The one Unix-seconds clock of lens.
pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The hourly retention sweep of the history store.
pub(crate) async fn run_sweep_loop(store: Arc<HistoryStore>) {
    loop {
        tokio::time::sleep(SWEEP_INTERVAL).await;
        match store.sweep(now_unix()).await {
            Ok(n) => tracing::info!("history sweep deleted {n} entries"),
            Err(e) => tracing::warn!("history sweep error: {e}"),
        }
    }
}
