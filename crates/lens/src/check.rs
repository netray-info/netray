use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::backends::{BackendContext, BackendExtra, BackendResult};
use crate::scoring::engine::{OverallScore, SectionInput, SectionStatus, compute_score};
use crate::state::AppState;
use futures::StreamExt;

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
// Wave scheduling
// ---------------------------------------------------------------------------

/// Wave 1: run concurrently. No cross-section data dependencies.
const WAVE1_SECTIONS: &[&str] = &["dns", "tls", "http", "email"];

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
    let deadline = tokio::time::Instant::now() + hard_deadline;
    let domain = input.domain.clone();
    let mut sections: Sections = HashMap::new();

    // Wave 1: run concurrently, each result recorded as soon as it finishes.
    let wave1_context = BackendContext {
        resolved_ips: vec![],
        dkim_selectors: input.dkim_selectors.clone(),
        forward_headers: Default::default(),
    };
    run_wave(
        state,
        WAVE1_SECTIONS,
        &domain,
        &wave1_context,
        deadline,
        &mut sections,
    )
    .await;

    // Extract resolved IPs from DNS result.
    let resolved_ips: Vec<IpAddr> = sections
        .get("dns")
        .and_then(|r| r.as_ref().ok())
        .and_then(|br| match &br.extra {
            BackendExtra::Dns { resolved_ips, .. } => Some(resolved_ips.clone()),
            _ => None,
        })
        .unwrap_or_default();

    // Wave 2: run after wave 1, within the remaining time.
    let wave2_context = BackendContext {
        resolved_ips,
        dkim_selectors: None,
        forward_headers: Default::default(),
    };
    run_wave(
        state,
        WAVE2_SECTIONS,
        &domain,
        &wave2_context,
        deadline,
        &mut sections,
    )
    .await;

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

/// Run the backends of one wave concurrently. Each result lands in `sections` as soon as its
/// backend finishes; when `deadline` passes, the backends still running are dropped and
/// recorded as `Timeout`.
async fn run_wave(
    state: &AppState,
    wave: &[&str],
    domain: &str,
    ctx: &BackendContext,
    deadline: tokio::time::Instant,
    sections: &mut Sections,
) {
    let mut pending = futures::stream::FuturesUnordered::new();
    let mut expected: Vec<String> = Vec::new();
    for b in state
        .backends
        .iter()
        .filter(|b| wave.contains(&b.section()))
    {
        let section = b.section().to_string();
        expected.push(section.clone());
        pending.push(async move { (section, b.run(domain, ctx).await) });
    }

    while !pending.is_empty() {
        match tokio::time::timeout_at(deadline, pending.next()).await {
            Ok(Some((section, result))) => {
                sections.insert(section, result);
            }
            Ok(None) => break,
            Err(_elapsed) => break,
        }
    }
    drop(pending);

    for section in expected {
        sections
            .entry(section)
            .or_insert(Err(SectionError::Timeout));
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
