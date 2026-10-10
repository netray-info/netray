use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};

use crate::backend::Ifconfig;
use crate::module::{ip_checks, sample, translate};

struct GoldenModule(String);

impl Module for GoldenModule {
    fn protocol(&self) -> Protocol {
        Protocol::Ip
    }

    fn checks(&self) -> &'static [CheckId] {
        ip_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn needs_addresses(&self) -> bool {
        true
    }

    fn run<'a>(&'a self, _ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            let (sample, total) = sample(facts);
            let lookups: Vec<_> = sample.into_iter().map(|ip| (ip, Ok(decode(&self.0)))).collect();
            translate(&lookups, total)
        })
    }
}

fn decode(contract_json: &str) -> Ifconfig {
    serde_json::from_str(contract_json).expect("golden contract decodes as an ifconfig response")
}

/// An IP module that answers every sampled address with the decoded `contract_json`, an
/// ifconfig `/json` response.
pub fn golden_module(contract_json: &str) -> Box<dyn Module> {
    decode(contract_json);
    Box::new(GoldenModule(contract_json.to_string()))
}
