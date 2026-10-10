use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};

use crate::inspect::assembler::InspectResponse;
use crate::module::{http_checks, translate};

struct GoldenModule(SectionOutcome);

impl Module for GoldenModule {
    fn protocol(&self) -> Protocol {
        Protocol::Http
    }

    fn checks(&self) -> &'static [CheckId] {
        http_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move { self.0.clone() })
    }
}

/// An HTTP module that answers every run with the translation of `contract_json`, a spectra
/// inspect response.
pub fn golden_module(contract_json: &str) -> Box<dyn Module> {
    let resp: InspectResponse = serde_json::from_str(contract_json)
        .expect("golden contract decodes as an inspect response");
    Box::new(GoldenModule(translate(&resp)))
}
