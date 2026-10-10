use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use serde::de::DeserializeOwned;

use crate::api::CheckEvent;
use crate::module::{dns_checks, translate};

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
    let events: Vec<CheckEvent> = contract_sse
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
        .collect();
    Box::new(GoldenModule(translate(&events)))
}

fn decode<T: DeserializeOwned>(data: &str) -> T {
    serde_json::from_str(data).expect("golden frame decodes as a prism check event")
}
