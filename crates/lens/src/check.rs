use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::backends::dns::DnsBackendResult;
use crate::backends::ip::IpBackendResult;
use crate::backends::tls::TlsBackendResult;
use crate::scoring::engine::{CheckResult, OverallScore, SectionInput, compute_score};
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
}

/// Output of a full domain health check.
pub struct CheckOutput {
    pub domain: String,
    pub dns: Result<DnsBackendResult, SectionError>,
    pub tls: Result<TlsBackendResult, SectionError>,
    pub ip: Result<IpBackendResult, SectionError>,
    pub score: OverallScore,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

/// Run a full domain health check against the configured backends.
///
/// Flow:
/// 1. DNS and TLS run concurrently with `tokio::join!`.
/// 2. Resolved IPs from DNS are passed to the IP backend.
/// 3. A 20-second hard deadline wraps everything.
/// 4. Each section independently captures errors — one failure never aborts the others.
/// 5. Score is computed from whatever results are available.
pub async fn run_check(state: &AppState, domain: &str) -> CheckOutput {
    let start = Instant::now();
    let config = &state.config;
    let timeout = Duration::from_secs(config.backends.backend_timeout_secs);
    // Hard overall deadline: slightly above the per-backend timeout so backends
    // have a chance to complete, but we never block indefinitely.
    let hard_deadline = Duration::from_secs(20);

    let result = tokio::time::timeout(
        hard_deadline,
        run_backends(state, domain, timeout),
    )
    .await;

    match result {
        Ok(output) => output,
        Err(_elapsed) => {
            // Hard deadline fired — return Timeout for all sections.
            let score = build_score_from_errors(state);
            CheckOutput {
                domain: domain.to_string(),
                dns: Err(SectionError::Timeout),
                tls: Err(SectionError::Timeout),
                ip: Err(SectionError::Timeout),
                score,
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
    }
}

/// Inner async function that runs the actual backend calls.
///
/// Separated from `run_check` so the hard timeout can wrap it cleanly.
async fn run_backends(state: &AppState, domain: &str, timeout: Duration) -> CheckOutput {
    let start = Instant::now();
    let config = &state.config;
    let client = &state.http_client;

    // Step 1: DNS and TLS in parallel.
    let (dns_result, tls_result) = tokio::join!(
        crate::backends::dns::check_dns(
            client,
            &config.backends.dns_url,
            domain,
            timeout,
        ),
        crate::backends::tls::check_tls(
            client,
            &config.backends.tls_url,
            domain,
            timeout,
        ),
    );

    // Step 2: Extract IPs from DNS result (empty vec if DNS errored).
    let resolved_ips: Vec<IpAddr> = match &dns_result {
        Ok(dns) => dns.resolved_ips.clone(),
        Err(_) => vec![],
    };

    // Step 3: IP enrichment (needs resolved IPs from DNS).
    let ip_result = crate::backends::ip::check_ip(
        client,
        &config.backends.ip_url,
        &resolved_ips,
        timeout,
    )
    .await;

    // Step 4: Map AppError to SectionError.
    let dns = map_error(dns_result);
    let tls = map_error(tls_result);
    let ip = map_error(ip_result);

    // Step 5: Build scoring inputs.
    let dns_input = section_input_from_result(&dns);
    let tls_input = section_input_from_result(&tls);
    let ip_input = section_input_from_result(&ip);

    // Step 6: Compute score.
    let score = compute_score(&state.scoring_profile, dns_input, tls_input, ip_input);

    CheckOutput {
        domain: domain.to_string(),
        dns,
        tls,
        ip,
        score,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn map_error<T>(
    result: Result<T, crate::error::AppError>,
) -> Result<T, SectionError> {
    result.map_err(|e| match e {
        crate::error::AppError::Timeout => SectionError::Timeout,
        other => SectionError::BackendError(other.to_string()),
    })
}

/// Build a SectionInput for the scoring engine from a backend result.
///
/// - If the result is Ok, use the checks from the backend result.
/// - If the result is Err, mark the section as errored (excluded from scoring).
fn section_input_from_result<T: HasChecks>(result: &Result<T, SectionError>) -> SectionInput {
    match result {
        Ok(r) => SectionInput {
            checks: r.checks().to_vec(),
            errored: false,
        },
        Err(_) => SectionInput {
            checks: vec![],
            errored: true,
        },
    }
}

/// Trait to uniformly extract checks from the three different backend result types.
trait HasChecks {
    fn checks(&self) -> &[CheckResult];
}

impl HasChecks for DnsBackendResult {
    fn checks(&self) -> &[CheckResult] {
        &self.checks
    }
}

impl HasChecks for TlsBackendResult {
    fn checks(&self) -> &[CheckResult] {
        &self.checks
    }
}

impl HasChecks for IpBackendResult {
    fn checks(&self) -> &[CheckResult] {
        &self.checks
    }
}

/// Build a score from all-errored sections (used when the hard deadline fires).
fn build_score_from_errors(state: &AppState) -> OverallScore {
    compute_score(
        &state.scoring_profile,
        SectionInput { checks: vec![], errored: true },
        SectionInput { checks: vec![], errored: true },
        SectionInput { checks: vec![], errored: true },
    )
}
