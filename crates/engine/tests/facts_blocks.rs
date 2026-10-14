//! v2-evidence requirement 3a: the facts blocks are part of the run even when the DNS section
//! timed out, so a check in another module that references `dns.caa` still resolves. Stub
//! modules and a stub FactsProvider only.

use std::time::{Duration, Instant};

use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, RunOptions, SectionEvent, SectionOutcome, run,
};
use netray_model::evidence::{Block, BlockId, Shape, check_evidence_rule};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use tokio::sync::mpsc;

const MS: fn(u64) -> Duration = Duration::from_millis;

/// Overruns its section deadline.
struct HangingDns;

impl Module for HangingDns {
    fn protocol(&self) -> Protocol {
        Protocol::Dns
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async {
            tokio::time::sleep(MS(5000)).await;
            SectionOutcome::TimedOut
        })
    }
}

/// One passing `tls.caa_compliant` that references `dns.caa`.
struct Tls;

impl Module for Tls {
    fn protocol(&self) -> Protocol {
        Protocol::Tls
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn wants_facts(&self) -> bool {
        true
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async {
            SectionOutcome::Measured {
                checks: vec![CheckResult {
                    id: CheckId::parse("tls.caa_compliant").unwrap(),
                    status: Status::Pass,
                    findings: vec![],
                    evidence: vec![BlockId::parse("dns.caa").unwrap()],
                }],
                evidence: vec![],
            }
        })
    }
}

struct StubProvider(Facts);

impl FactsProvider for StubProvider {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        let facts = self.0.clone();
        Box::pin(async move { Ok(facts) })
    }
}

#[tokio::test]
async fn facts_blocks_survive_a_dns_timeout_and_resolve_references() {
    let caa = Block {
        id: BlockId::parse("dns.caa").unwrap(),
        title: "CAA".into(),
        shape: Shape::Records(vec![]),
    };
    let facts = Facts {
        blocks: vec![caa],
        ..Facts::default()
    };
    let registry = Registry::new()
        .with_facts(Box::new(StubProvider(facts)))
        .with(Box::new(HangingDns))
        .with(Box::new(Tls));
    let base = RunContext {
        deadline: Instant::now() + MS(2000),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let sections = [(Protocol::Dns, MS(200)), (Protocol::Tls, MS(1000))];
    let (tx, mut rx) = mpsc::channel(8);
    let collect = async {
        let mut events: Vec<SectionEvent> = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        events
    };
    let (report, events) = tokio::join!(run(&registry, base, &sections, MS(500), tx), collect);

    let dns = events.iter().find(|e| e.protocol == Protocol::Dns).unwrap();
    assert_eq!(dns.outcome, SectionOutcome::TimedOut);

    let blocks: Vec<Block> = report.blocks().into_iter().cloned().collect();
    let want = BlockId::parse("dns.caa").unwrap();
    assert!(
        blocks.iter().any(|b| b.id == want),
        "report.blocks() holds dns.caa"
    );

    let checks: Vec<CheckResult> = events
        .iter()
        .filter_map(|e| match &e.outcome {
            SectionOutcome::Measured { checks, .. } => Some(checks.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(checks.len(), 1);
    assert_eq!(check_evidence_rule(&checks, &blocks), Ok(()));
}
