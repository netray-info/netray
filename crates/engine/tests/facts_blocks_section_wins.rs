//! v2-evidence C11 (operator decision 2026-10-13): where the facts and the DNS section carry the
//! same block id, the section's block wins; when the section timed out the facts block stays.

use std::time::{Duration, Instant};

use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, RunOptions, SectionOutcome, run,
};
use netray_model::evidence::{Block, BlockId, Record, Shape, Value};
use netray_model::{CheckId, Protocol};
use tokio::sync::mpsc;

const MS: fn(u64) -> Duration = Duration::from_millis;

/// A DNS section that returns `outcome` (after `delay`).
struct Dns {
    outcome: SectionOutcome,
    delay: Duration,
}

impl Module for Dns {
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
        let outcome = self.outcome.clone();
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            outcome
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

fn caa(records: Vec<Record>) -> Block {
    Block {
        id: BlockId::parse("dns.caa").unwrap(),
        title: "CAA".into(),
        shape: Shape::Records(records),
    }
}

fn one_record() -> Record {
    Record {
        owner: Value::Text("example.com.".into()),
        rtype: Value::Text("CAA".into()),
        ttl: Value::Int(300),
        data: Value::Text("0 issue \"ca.example.com\"".into()),
    }
}

async fn caa_blocks(section: Dns, section_budget: Duration) -> Vec<Block> {
    let registry = Registry::new()
        .with_facts(Box::new(StubProvider(Facts {
            blocks: vec![caa(vec![])],
            ..Facts::default()
        })))
        .with(Box::new(section));
    let base = RunContext {
        deadline: Instant::now() + MS(2000),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let sections = [(Protocol::Dns, section_budget)];
    let (tx, mut rx) = mpsc::channel(8);
    let drain = async { while rx.recv().await.is_some() {} };
    let (report, ()) = tokio::join!(run(&registry, base, &sections, MS(500), tx), drain);
    let id = BlockId::parse("dns.caa").unwrap();
    report
        .blocks()
        .into_iter()
        .filter(|b| b.id == id)
        .cloned()
        .collect()
}

#[tokio::test]
async fn the_dns_section_block_wins_over_the_facts_block_of_the_same_id() {
    let section_block = caa(vec![one_record()]);
    let measured = Dns {
        outcome: SectionOutcome::Measured {
            checks: vec![],
            evidence: vec![section_block.clone()],
        },
        delay: MS(0),
    };
    assert_eq!(
        caa_blocks(measured, MS(1000)).await,
        vec![section_block],
        "exactly one dns.caa, the section's with one record"
    );

    // The section timed out: the facts block stays.
    let hanging = Dns {
        outcome: SectionOutcome::TimedOut,
        delay: MS(5000),
    };
    assert_eq!(
        caa_blocks(hanging, MS(200)).await,
        vec![caa(vec![])],
        "timed-out section: the facts dns.caa is present"
    );
}
