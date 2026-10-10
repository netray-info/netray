use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};

use crate::module::{email_checks, translate};
use crate::quality::types::SseEvent;

struct GoldenModule(SectionOutcome);

impl Module for GoldenModule {
    fn protocol(&self) -> Protocol {
        Protocol::Email
    }

    fn checks(&self) -> &'static [CheckId] {
        email_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move { self.0.clone() })
    }
}

/// An email module that answers every run with the translation of `contract_sse`, the `data:`
/// lines of a beacon SSE stream.
pub fn golden_module(contract_sse: &str) -> Box<dyn Module> {
    let events: Vec<SseEvent> = contract_sse
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(|d| serde_json::from_str(d.trim()).expect("golden contract decodes as SSE events"))
        .collect();
    Box::new(GoldenModule(translate(&events)))
}
