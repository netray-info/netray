use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::backends::{BackendContext, BackendExtra, BackendResult};
use crate::scoring::engine::{OverallScore, SectionInput, compute_score};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct CheckInput {
    pub domain: String,
}

/// Error that can occur for a single backend section.
#[derive(Debug, Clone)]
pub enum SectionError {
    BackendError(String),
    Timeout,
    NoDnsResults,
}

/// Output of a full domain health check.
pub struct CheckOutput {
    pub domain: String,
    pub sections: HashMap<String, Result<BackendResult, SectionError>>,
    pub score: OverallScore,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Wave scheduling
// ---------------------------------------------------------------------------

/// Wave 1: run concurrently. No cross-section data dependencies.
const WAVE1_SECTIONS: &[&str] = &["dns", "tls", "http"];

/// Wave 2: run after wave 1. IP backend needs resolved IPs from DNS.
const WAVE2_SECTIONS: &[&str] = &["ip"];

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

/// Run a full domain health check against the configured backends.
///
/// Flow:
/// 1. DNS and TLS run concurrently (wave 1) with `tokio::join!`.
/// 2. Resolved IPs from DNS are passed to the IP backend (wave 2).
/// 3. A 20-second hard deadline wraps everything.
/// 4. Each section independently captures errors — one failure never aborts the others.
/// 5. Score is computed from whatever results are available.
pub async fn run_check(state: &AppState, domain: &str) -> CheckOutput {
    let start = Instant::now();
    let hard_deadline = Duration::from_secs(20);

    let result = tokio::time::timeout(hard_deadline, run_backends(state, domain)).await;

    match result {
        Ok(output) => output,
        Err(_elapsed) => {
            // Hard deadline fired — return Timeout for all sections.
            let score = build_score_from_errors(state);
            let mut sections = HashMap::new();
            for backend in state.backends.iter() {
                sections.insert(backend.section().to_string(), Err(SectionError::Timeout));
            }
            CheckOutput {
                domain: domain.to_string(),
                sections,
                score,
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
    }
}

/// Inner async function that runs the actual backend calls.
async fn run_backends(state: &AppState, domain: &str) -> CheckOutput {
    let start = Instant::now();
    let mut sections: HashMap<String, Result<BackendResult, SectionError>> = HashMap::new();

    // Wave 1: run concurrently.
    let wave1_context = BackendContext {
        resolved_ips: vec![],
    };
    let wave1_futures: Vec<_> = state
        .backends
        .iter()
        .filter(|b| WAVE1_SECTIONS.contains(&b.section()))
        .map(|b| {
            let section = b.section().to_string();
            let ctx = wave1_context.clone();
            let domain = domain.to_string();
            async move {
                let result = b.run(&domain, &ctx).await;
                (section, result)
            }
        })
        .collect();

    let wave1_results = futures::future::join_all(wave1_futures).await;

    for (section, result) in wave1_results {
        sections.insert(section, result);
    }

    // Extract resolved IPs from DNS result.
    let resolved_ips: Vec<IpAddr> = sections
        .get("dns")
        .and_then(|r| r.as_ref().ok())
        .and_then(|br| match &br.extra {
            BackendExtra::Dns { resolved_ips, .. } => Some(resolved_ips.clone()),
            _ => None,
        })
        .unwrap_or_default();

    // Wave 2: run after wave 1.
    let wave2_context = BackendContext { resolved_ips };
    for backend in state.backends.iter() {
        if WAVE2_SECTIONS.contains(&backend.section()) {
            let result = backend.run(domain, &wave2_context).await;
            sections.insert(backend.section().to_string(), result);
        }
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
        domain: domain.to_string(),
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
            errored: false,
        },
        Err(_) => SectionInput {
            checks: vec![],
            errored: true,
        },
    }
}

/// Build a score from all-errored sections (used when the hard deadline fires).
fn build_score_from_errors(state: &AppState) -> OverallScore {
    let mut inputs: HashMap<String, SectionInput> = HashMap::new();
    for name in state.scoring_profile.sections.keys() {
        inputs.insert(
            name.clone(),
            SectionInput {
                checks: vec![],
                errored: true,
            },
        );
    }
    compute_score(&state.scoring_profile, &inputs)
}
