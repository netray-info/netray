use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use netray_engine::{Domain, RunContext, RunOptions};
use netray_model::Protocol;
use tokio::sync::mpsc;

use crate::modules::{Backend, BackendResult};
use crate::scoring::engine::{OverallScore, SectionInput, SectionStatus, compute_score};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct CheckInput {
    pub domain: String,
    pub dkim_selectors: Option<Vec<String>>,
    /// Resolved client IP, forwarded to backends as `X-Forwarded-For`.
    pub client_ip: Option<IpAddr>,
    /// Inbound request ID, forwarded to backends as `X-Request-Id`.
    pub request_id: Option<String>,
}

/// Error that can occur for a single backend section.
#[derive(Debug, Clone)]
pub enum SectionError {
    BackendError(String),
    Timeout,
    NoDnsResults,
    NotApplicable { reason: String },
}

/// Output of a full domain health check.
pub struct CheckOutput {
    pub domain: String,
    pub sections: HashMap<String, Result<BackendResult, SectionError>>,
    pub score: OverallScore,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

/// Run a full domain health check against the configured backends.
///
/// Flow:
/// 1. One engine run: the registry's resolve stage under `[backends] resolve_timeout_ms`,
///    concurrently with every configured section.
/// 2. A 20-second hard deadline wraps everything.
/// 3. Each section independently captures errors — one failure never aborts the others.
/// 4. Score is computed from whatever results are available.
pub async fn run_check(state: &AppState, domain: &str) -> CheckOutput {
    run_check_with_input(
        state,
        CheckInput {
            domain: domain.to_string(),
            dkim_selectors: None,
            client_ip: None,
            request_id: None,
        },
    )
    .await
}

/// Hard deadline for a whole check. The configured backend timeouts must sum below it.
pub const HARD_DEADLINE: Duration = Duration::from_secs(20);

pub async fn run_check_with_input(state: &AppState, input: CheckInput) -> CheckOutput {
    run_check_with_deadline(state, input, HARD_DEADLINE).await
}

type Sections = HashMap<String, Result<BackendResult, SectionError>>;

/// Run the check under `hard_deadline`. Sections that finished before it fires are kept; only
/// the unfinished ones become `Err(SectionError::Timeout)`.
pub async fn run_check_with_deadline(
    state: &AppState,
    input: CheckInput,
    hard_deadline: Duration,
) -> CheckOutput {
    let start = Instant::now();
    let domain = input.domain.clone();
    let base = RunContext {
        deadline: start + hard_deadline,
        domain: Domain::new(domain.as_str()),
        options: RunOptions {
            dkim_selectors: input.dkim_selectors.clone(),
        },
    };
    let plan: Vec<(Protocol, Duration)> = state
        .backends
        .iter()
        .map(|s| (s.protocol, s.timeout))
        .collect();

    let resolve_budget = Duration::from_millis(state.config.backends.resolve_timeout_ms);

    let (tx, mut rx) = mpsc::channel(plan.len().max(1));
    let mut events = Vec::new();
    let (report, ()) = tokio::join!(
        netray_engine::run(&state.registry, base, &plan, resolve_budget, tx),
        async {
            while let Some(event) = rx.recv().await {
                events.push(event);
            }
        }
    );

    if let Some(e) = &report.resolve_error {
        tracing::warn!(error = %e, "address resolution failed");
    }

    let mut sections: Sections = HashMap::new();
    for event in events {
        if let Some(s) = state.backends.iter().find(|s| s.protocol == event.protocol) {
            sections.insert(
                s.section().to_string(),
                s.adapt(&domain, event.outcome, &report),
            );
        }
    }
    for s in state.backends.iter() {
        sections
            .entry(s.section().to_string())
            .or_insert(Err(SectionError::Timeout));
    }

    // Build scoring inputs.
    let mut inputs: HashMap<String, SectionInput> = HashMap::new();
    for (name, result) in &sections {
        inputs.insert(name.clone(), section_input_from_result(result));
    }

    // Warn about profile sections with no registered backend.
    for name in state.scoring_profile.sections.keys() {
        if !sections.contains_key(name) {
            tracing::warn!(section = %name, "profile section has no registered backend — skipped");
        }
    }

    let score = compute_score(&state.scoring_profile, &inputs);

    CheckOutput {
        domain,
        sections,
        score,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a SectionInput for the scoring engine from a backend result.
fn section_input_from_result(result: &Result<BackendResult, SectionError>) -> SectionInput {
    match result {
        Ok(r) => SectionInput {
            checks: r.checks.clone(),
            status: SectionStatus::Scored,
        },
        Err(SectionError::NotApplicable { reason }) => SectionInput {
            checks: vec![],
            status: SectionStatus::NotApplicable {
                reason: reason.clone(),
            },
        },
        Err(_) => SectionInput {
            checks: vec![],
            status: SectionStatus::Errored,
        },
    }
}
