use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Instant;

use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};

use url::{Host, Url};

use crate::error::AppError;
use crate::inspect::assembler::InspectResponse;
use crate::job::HttpInspect;
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

const SPECTRA_JSON: &str = include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

/// An offline `HttpInspect`: names resolve from a map (default `93.184.215.14`), the real target
/// policy applies, and every inspection answers the spectra contract fixture.
#[derive(Default)]
pub struct StubHttpInspect {
    hosts: HashMap<String, Vec<IpAddr>>,
}

impl StubHttpInspect {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_host(mut self, name: &str, addrs: Vec<IpAddr>) -> Self {
        self.hosts.insert(name.to_lowercase(), addrs);
        self
    }
}

impl HttpInspect for StubHttpInspect {
    fn target<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<SocketAddr, AppError>> {
        Box::pin(async move {
            let addrs = match url.host() {
                Some(Host::Ipv4(ip)) => vec![IpAddr::V4(ip)],
                Some(Host::Ipv6(ip)) => vec![IpAddr::V6(ip)],
                Some(Host::Domain(name)) => self
                    .hosts
                    .get(&name.to_lowercase())
                    .cloned()
                    .unwrap_or_else(|| vec![IpAddr::V4(Ipv4Addr::new(93, 184, 215, 14))]),
                None => return Err(AppError::InvalidUrl("URL has no host".to_string())),
            };
            crate::input::pick_target(&addrs, url.port_or_known_default().unwrap_or(443))
        })
    }

    fn inspect<'a>(
        &'a self,
        url: &'a Url,
        _target: SocketAddr,
        _start: Instant,
    ) -> BoxFuture<'a, Result<InspectResponse, AppError>> {
        Box::pin(async move {
            let mut resp: InspectResponse = serde_json::from_str(SPECTRA_JSON)
                .expect("the spectra contract fixture decodes as an inspect response");
            resp.url = url.to_string();
            Ok(resp)
        })
    }
}
