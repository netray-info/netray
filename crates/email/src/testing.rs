use std::sync::Arc;

use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};

use crate::config::Config;
use crate::dns::FetchResolver;
use crate::dns::test_support::TestDnsResolver;
use crate::job::EmailInspector;
use crate::module::{email_checks, translate};
use crate::quality::types::SseEvent;
use crate::state::OutboundFetch;

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

/// A resolver that answers `example.com` with one MX host, its address and PTR, an SPF and a
/// DMARC record, and every other name with a definite empty answer.
pub fn example_dns() -> TestDnsResolver {
    TestDnsResolver::new()
        .with_mx("example.com", vec![(10, "mx1.example.com")])
        .with_ips("mx1.example.com", vec!["192.0.2.10".parse().unwrap()])
        .with_ptr("192.0.2.10".parse().unwrap(), vec!["mx1.example.com"])
        .with_txt("example.com", vec!["v=spf1 mx -all"])
        .with_txt(
            "_dmarc.example.com",
            vec!["v=DMARC1; p=reject; rua=mailto:dmarc@example.com"],
        )
}

/// An email inspector over module defaults and `dns` for every lookup, offline.
pub fn stub_inspector(dns: TestDnsResolver) -> EmailInspector<TestDnsResolver> {
    let config = Config::from_module(serde_json::from_str("{}").expect("module defaults"));
    let dns = Arc::new(dns);
    let fetch = OutboundFetch::new(500, Arc::new(FetchResolver(dns.clone())));
    EmailInspector::new(Arc::new(config), dns.clone(), dns, fetch, None)
}
