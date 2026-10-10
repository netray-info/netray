use mhost::resolver::Lookups;
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, ResolveError, RunContext,
    SectionOutcome,
};
use netray_model::{CheckId, Protocol};
use serde::de::DeserializeOwned;

use crate::api::CheckEvent;
use crate::module::{dns_checks, facts_from_lookups, translate};

struct GoldenModule(SectionOutcome);

impl Module for GoldenModule {
    fn protocol(&self) -> Protocol {
        Protocol::Dns
    }

    fn checks(&self) -> &'static [CheckId] {
        dns_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move { self.0.clone() })
    }
}

/// A DNS module that answers every run with the translation of `contract_sse`, a prism check
/// stream of `event:`/`data:` frames. Frames other than `batch`, `lint` and `done` are skipped.
pub fn golden_module(contract_sse: &str) -> Box<dyn Module> {
    Box::new(GoldenModule(translate(&events(contract_sse))))
}

struct GoldenFacts(Facts);

impl FactsProvider for GoldenFacts {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

/// A facts provider that answers every resolve with `facts_from_lookups` of the merged batch
/// lookups of `contract_sse`, a prism check stream as for [`golden_module`].
pub fn golden_facts(contract_sse: &str) -> Box<dyn FactsProvider> {
    let lookups = events(contract_sse)
        .into_iter()
        .filter_map(|event| match event {
            CheckEvent::Batch(batch) => Some(batch.lookups),
            _ => None,
        })
        .fold(Lookups::empty(), Lookups::merge);
    Box::new(GoldenFacts(facts_from_lookups(&lookups)))
}

fn events(contract_sse: &str) -> Vec<CheckEvent> {
    contract_sse
        .split("\n\n")
        .filter_map(|block| {
            let name = block.lines().find_map(|l| l.strip_prefix("event:"))?.trim();
            let data = block.lines().find_map(|l| l.strip_prefix("data:"))?.trim();
            Some(match name {
                "batch" => CheckEvent::Batch(decode(data)),
                "lint" => CheckEvent::Lint(decode(data)),
                "done" => CheckEvent::Done(decode(data)),
                _ => return None,
            })
        })
        .collect()
}

fn decode<T: DeserializeOwned>(data: &str) -> T {
    serde_json::from_str(data).expect("golden frame decodes as a prism check event")
}
