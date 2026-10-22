//! Enrichment failures must not score as healthy (spec grade-integrity; criteria C5, C6), on the
//! domain run (copied from lens's `tests/ip_enrichment_errors.rs`, spec v2-cutover Phase 6;
//! assertions unchanged).
//!
//! A failed or stalled IP lookup makes the IP section Errored
//! (`Err(SectionError::BackendError | Timeout)`), which is what makes the overall result
//! incomplete. The lookup failure is the module's `Incomplete` outcome and a stall is a module
//! that does not finish within its section window. lens used the IP module's contract golden for
//! the answering module; the engine names no module crate, so a stub IP module that answers a
//! measured section stands in for it.

use std::time::Duration;

use netray_engine::domain_run::{
    BackendResult, CheckInput, HARD_DEADLINE, SectionError, run_check_streaming,
};
use netray_engine::profile::Profiles;
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, SectionOutcome,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};

const PUBLIC: [&str; 2] = ["1.1.1.1", "8.8.8.8"];

/// An IP module that needs addresses and answers `outcome` after `delay` (`None`: never).
struct Ip {
    outcome: SectionOutcome,
    delay: Option<Duration>,
}

impl Module for Ip {
    fn protocol(&self) -> Protocol {
        Protocol::Ip
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn needs_addresses(&self) -> bool {
        true
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            match self.delay {
                Some(d) => tokio::time::sleep(d).await,
                None => std::future::pending::<()>().await,
            }
            self.outcome.clone()
        })
    }
}

fn ip_incomplete() -> Ip {
    Ip {
        outcome: SectionOutcome::Incomplete {
            reason: "module failed".to_string(),
        },
        delay: Some(Duration::ZERO),
    }
}

fn ip_measured() -> Ip {
    Ip {
        outcome: SectionOutcome::Measured {
            checks: vec![CheckResult {
                id: CheckId::parse("ip.reputation").unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            }],
            evidence: vec![],
        },
        delay: Some(Duration::ZERO),
    }
}

struct StubFacts(Facts);

impl FactsProvider for StubFacts {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

/// The IP section of a run over `module` for a domain whose resolve stage answered `ips`, as
/// lens's `run_ip` ran it: the IP section alone, the resolve under the hard deadline.
async fn run_ip(
    module: Ip,
    timeout: Duration,
    ips: &[&str],
) -> Result<BackendResult, SectionError> {
    let mut facts = Facts::default();
    for ip in ips {
        match ip.parse::<std::net::IpAddr>().unwrap() {
            std::net::IpAddr::V4(a) => facts.a.push(a),
            std::net::IpAddr::V6(a) => facts.aaaa.push(a),
        }
    }
    let registry = Registry::new()
        .with(Box::new(module))
        .with_facts(Box::new(StubFacts(facts)));
    let (mut output, _) = run_check_streaming(
        &registry,
        &[(Protocol::Ip, timeout)],
        HARD_DEADLINE,
        Profiles::current(),
        CheckInput {
            domain: "example.com".to_string(),
            dkim_selectors: None,
        },
        HARD_DEADLINE,
        |_| {},
    )
    .await;
    output.sections.remove("ip").expect("the ip section")
}

fn is_errored<T>(r: &Result<T, SectionError>) -> bool {
    matches!(
        r,
        Err(SectionError::BackendError(_) | SectionError::Timeout)
    )
}

#[tokio::test]
async fn c5_a_failed_enrichment_errors_the_ip_section() {
    let run = run_ip(ip_incomplete(), Duration::from_secs(5), &PUBLIC).await;
    assert!(is_errored(&run), "IP section must be Errored");
}

#[tokio::test]
async fn c6_a_stalled_enrichment_errors_the_ip_section() {
    let stalled = Ip {
        delay: None,
        ..ip_measured()
    };
    let run = run_ip(stalled, Duration::from_millis(300), &PUBLIC).await;
    assert!(
        matches!(run, Err(SectionError::Timeout)),
        "IP section must be Errored by timeout"
    );
}

#[tokio::test]
async fn a_healthy_enrichment_does_not_error_the_ip_section() {
    let run = run_ip(ip_measured(), Duration::from_secs(5), &PUBLIC).await;
    assert!(run.is_ok(), "the measured module answers every address");
}

/// lens's `modules.rs` unit test `ip_section_returns_no_dns_results_when_empty`: a resolve stage
/// that answered no address and no definite negative makes the IP section `NoDnsResults`.
#[tokio::test]
async fn ip_section_returns_no_dns_results_when_empty() {
    let result = run_ip(ip_measured(), Duration::from_secs(5), &[]).await;
    assert!(
        matches!(result, Err(SectionError::NoDnsResults)),
        "expected NoDnsResults, got: {result:?}"
    );
}
