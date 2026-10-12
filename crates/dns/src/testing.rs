use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use hickory_proto::op::{Message, MessageType, OpCode};
use hickory_proto::rr::rdata::{NS, SOA};
use hickory_proto::rr::{Name, RData, Record, RecordType};
use mhost::resolver::Lookups;
use netray_common::fetch::Resolve;
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, ResolveError, RunContext,
    SectionOutcome,
};
use netray_model::{CheckId, Protocol};
use serde::de::DeserializeOwned;

use crate::api::CheckEvent;
use crate::api::check::{
    LintEvent, check_ns_delegation_consistency, check_ns_lame_delegation, unanswered_events,
    unique_lines,
};
use crate::dns_raw::{RawError, RawOutbound, RawResponse, RawSend, SendFuture};
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

/// How a stubbed nameserver behaves for [`ns_check_outcomes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NsStub {
    /// Answers SOA with AA=1 and NS with the full name set.
    Authoritative,
    /// Never answers.
    Timeout,
    /// The host name does not resolve.
    NxDomain,
    /// Every query fails with a network error.
    NetError,
    /// The host resolves to 10.0.0.1, which the outbound policy refuses.
    Private,
}

/// The `dns.ns_lame` and `dns.ns_delegation` results of the production NS checks for `domain`,
/// whose NS records are the hosts of `ns`, each answering as its stub. Host `n` (from 1) has
/// the address 192.0.2.n, a `Private` host 10.0.0.1. No network.
pub async fn ns_check_outcomes(
    domain: &str,
    ns: &[(&str, NsStub)],
) -> Vec<netray_model::CheckResult> {
    let fqdn = |host: &str| format!("{}.", host.trim_end_matches('.'));
    let hosts: Vec<(String, IpAddr, NsStub)> = ns
        .iter()
        .zip(1u8..)
        .map(|((host, stub), n)| {
            let ip = match stub {
                NsStub::Private => Ipv4Addr::new(10, 0, 0, 1),
                _ => Ipv4Addr::new(192, 0, 2, n),
            };
            (
                host.trim_end_matches('.').to_string(),
                IpAddr::V4(ip),
                *stub,
            )
        })
        .collect();
    let records: Vec<serde_json::Value> = hosts
        .iter()
        .map(|(host, _, _)| {
            serde_json::json!({
                "name": fqdn(domain), "type": "NS", "ttl": 300, "data": { "NS": fqdn(host) }
            })
        })
        .collect();
    let lookups: Lookups = serde_json::from_value(serde_json::json!({
        "lookups": [{
            "query": { "name": fqdn(domain), "record_type": "NS" },
            "name_server": "udp:192.0.2.53:53",
            "result": { "Response": {
                "records": records,
                "response_time": { "secs": 0, "nanos": 10_000_000 },
                "valid_until": "2099-01-01T00:00:00Z"
            }}
        }]
    }))
    .expect("NS lookups decode");

    let raw = RawOutbound {
        allow: |ip| !matches!(ip, IpAddr::V4(v4) if v4.is_private()),
        resolver: Arc::new(StubResolve(
            hosts
                .iter()
                .filter(|(_, _, stub)| *stub != NsStub::NxDomain)
                .map(|(host, ip, _)| (host.clone(), *ip))
                .collect(),
        )),
        sender: Arc::new(StubSend {
            zone: fqdn(domain),
            ns_names: hosts.iter().map(|(host, _, _)| fqdn(host)).collect(),
            stubs: hosts.iter().map(|(_, ip, stub)| (*ip, *stub)).collect(),
        }),
    };
    let timeout = Duration::from_secs(1);
    let (lame, delegation) = tokio::join!(
        check_ns_lame_delegation(&lookups, domain, timeout, &raw),
        check_ns_delegation_consistency(&lookups, domain, timeout, &raw),
    );

    let mut events: Vec<CheckEvent> = [("ns_lame", &lame), ("ns_delegation", &delegation)]
        .into_iter()
        .map(|(category, check)| {
            CheckEvent::Lint(LintEvent {
                request_id: String::new(),
                category,
                results: unique_lines(check.results.clone()),
            })
        })
        .collect();
    events.extend(unanswered_events(&lame, &delegation));

    let SectionOutcome::Measured { checks, .. } = translate(&events) else {
        panic!("NS check events translate to a measured section");
    };
    checks
        .into_iter()
        .filter(|c| ["dns.ns_lame", "dns.ns_delegation"].contains(&c.id.to_string().as_str()))
        .collect()
}

struct StubResolve(HashMap<String, IpAddr>);

impl Resolve for StubResolve {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let ips: Vec<IpAddr> = self
            .0
            .get(host.trim_end_matches('.'))
            .copied()
            .into_iter()
            .collect();
        Box::pin(async move { ips })
    }
}

struct StubSend {
    zone: String,
    ns_names: Vec<String>,
    stubs: HashMap<IpAddr, NsStub>,
}

impl RawSend for StubSend {
    fn send(
        &self,
        server: SocketAddr,
        _name: Name,
        record_type: RecordType,
        _dnssec_ok: bool,
        timeout: Duration,
    ) -> SendFuture {
        let result = match self.stubs.get(&server.ip()) {
            Some(NsStub::Authoritative) => {
                let zone = Name::from_ascii(&self.zone).expect("zone name");
                let mut message = Message::new(0, MessageType::Response, OpCode::Query);
                message.metadata.authoritative = true;
                match record_type {
                    RecordType::SOA => {
                        let soa = SOA::new(
                            Name::from_ascii(&self.ns_names[0]).expect("NS name"),
                            Name::from_ascii(format!("hostmaster.{}", self.zone))
                                .expect("mailbox name"),
                            1,
                            7200,
                            3600,
                            1_209_600,
                            300,
                        );
                        message.add_answer(Record::from_rdata(zone, 3600, RData::SOA(soa)));
                    }
                    RecordType::NS => {
                        for ns in &self.ns_names {
                            let target = Name::from_ascii(ns).expect("NS name");
                            message.add_answer(Record::from_rdata(
                                zone.clone(),
                                3600,
                                RData::NS(NS(target)),
                            ));
                        }
                    }
                    _ => {}
                }
                Ok(RawResponse {
                    message,
                    latency: Duration::from_millis(1),
                })
            }
            Some(NsStub::NetError) => Err(RawError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "connection refused",
            ))),
            _ => Err(RawError::Timeout(timeout)),
        };
        Box::pin(async move { result })
    }
}
