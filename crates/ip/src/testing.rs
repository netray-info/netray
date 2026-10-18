use std::net::{IpAddr, SocketAddr};

use futures::future::BoxFuture;
use netray_engine::{EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};

use crate::backend::{Ifconfig, Tcp, UserAgent};
use crate::lookup::{AsnLookupResponse, IpLookup, IpRefusal, RangeResponse, parse_range};
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

const IFCONFIG_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/contracts/ifconfig-json.json"
));

/// An [`IpLookup`] that answers from the committed ifconfig `/json` fixture, with no data files and
/// no network.
#[derive(Debug, Default)]
pub struct StubIpLookup;

impl StubIpLookup {
    pub fn new() -> Self {
        Self
    }

    fn answer(ip: IpAddr) -> Ifconfig {
        let mut ifconfig = decode(IFCONFIG_FIXTURE);
        ifconfig.ip.addr = ip.to_string();
        ifconfig.ip.version = if ip.is_ipv4() { "4" } else { "6" }.to_string();
        ifconfig.tcp = None;
        ifconfig.user_agent = None;
        ifconfig
    }
}

impl IpLookup for StubIpLookup {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move { Ok(Self::answer(ip)) })
    }

    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move {
            let mut ifconfig = Self::answer(remote.ip());
            if remote.port() != 0 {
                ifconfig.tcp = Some(Tcp { port: remote.port() });
            }
            ifconfig.user_agent = user_agent.map(UserAgent::unparsed);
            Ok(ifconfig)
        })
    }

    fn range<'a>(&'a self, cidr: &'a str) -> BoxFuture<'a, Result<RangeResponse, IpRefusal>> {
        Box::pin(async move {
            parse_range(cidr)?;
            Ok(RangeResponse {
                cidr: cidr.to_string(),
                asn: Some(64496),
                org: Some("Example Cloud GmbH".to_string()),
                network_type: "cloud".to_string(),
                is_cloud: true,
                is_vpn: false,
                is_datacenter: true,
                is_spamhaus: false,
                is_c2: false,
            })
        })
    }

    fn asn(&self, n: u32) -> BoxFuture<'_, Result<AsnLookupResponse, IpRefusal>> {
        Box::pin(async move {
            Ok(AsnLookupResponse {
                asn: n,
                org: None,
                category: Some("hosting".to_string()),
                network_role: Some("stub".to_string()),
                is_anycast: false,
            })
        })
    }
}
