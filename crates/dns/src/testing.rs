use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use hickory_proto::op::{Message, MessageType, OpCode, ResponseCode};
use hickory_proto::rr::rdata::{A, CAA, HINFO, MX, NAPTR, NS, SOA, SRV, SSHFP, TLSA, TXT};
use hickory_proto::rr::{Name, RData, Record, RecordType};
use mhost::resolver::Lookups;
use netray_common::fetch::Resolve;
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, ResolveError, RunContext,
    SectionOutcome,
};
use netray_model::{CheckId, Protocol};
use serde::de::DeserializeOwned;

use crate::api::check::{
    LintEvent, check_ns_delegation_consistency, check_ns_lame_delegation, unanswered_events,
    unique_lines,
};
use crate::api::{AppState, CheckEvent, QUERY_SEMAPHORE_PERMITS};
use crate::config::{Config, ModuleConfig};
use crate::dns_raw::{RawError, RawOutbound, RawResponse, RawSend, SendFuture};
use crate::jobs::{DnsBackend, DnsJobs, ResolverLookup};
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

const ZONE: &str = "example.com";
const ZONE_TTL: u32 = 300;

fn in_zone(name: &str) -> bool {
    name.trim_end_matches('.').eq_ignore_ascii_case(ZONE)
}

/// Every lookup of this name answers SERVFAIL.
const SERVFAIL_NAME: &str = "dnssec-failed.org";
/// This name's A lookup answers the zone's address, its AAAA lookup SERVFAIL.
const PARTIAL_NAME: &str = "partial.example.net";

/// This name answers SRV, TLSA, SSHFP, NAPTR, HINFO and the zone's NS, recursively and
/// authoritatively.
const TYPES_NAME: &str = "types.example.com";
/// This name's NS are the zone's; its authoritative servers answer every query REFUSED.
const LAME_NAME: &str = "lame.example.com";
/// This name's A lookup answers 192.0.2.10 from the server at 8.8.8.8 and 192.0.2.20 from any
/// other.
const DIVERGE_NAME: &str = "diverge.example.com";
/// This name's SOA lookup answers a zone apex SOA with serial 2026101501.
const SOA_NAME: &str = "soa.example.com";

fn named(name: &str, wanted: &str) -> bool {
    name.trim_end_matches('.').eq_ignore_ascii_case(wanted)
}

fn types_data(record_type: &str) -> Vec<serde_json::Value> {
    match record_type {
        "SRV" => vec![serde_json::json!({ "SRV": {
            "priority": 10, "weight": 5, "port": 5060, "target": "sip.example.com."
        }})],
        "TLSA" => vec![serde_json::json!({ "TLSA": {
            "cert_usage": "DaneEe", "selector": "Spki", "matching": "Sha256",
            "cert_data": [0xab, 0xcd, 0x01]
        }})],
        "SSHFP" => vec![serde_json::json!({ "SSHFP": {
            "algorithm": "Ed25519", "fingerprint_type": "SHA256", "fingerprint": [0x12, 0xef]
        }})],
        "NAPTR" => vec![serde_json::json!({ "NAPTR": {
            "order": 100, "preference": 10, "flags": "S", "services": "SIP+D2U",
            "regexp": "", "replacement": "_sip._udp.example.com."
        }})],
        "HINFO" => vec![serde_json::json!({ "HINFO": { "cpu": "INTEL", "os": "LINUX" } })],
        "NS" => zone_data("NS"),
        _ => Vec::new(),
    }
}

fn types_records(record_type: RecordType) -> Vec<RData> {
    let host = |h: &str| Name::from_ascii(h).expect("zone host name");
    match record_type {
        RecordType::SRV => vec![RData::SRV(SRV::new(10, 5, 5060, host("sip.example.com.")))],
        RecordType::TLSA => vec![RData::TLSA(TLSA::new(
            3.into(),
            1.into(),
            1.into(),
            vec![0xab, 0xcd, 0x01],
        ))],
        RecordType::SSHFP => vec![RData::SSHFP(SSHFP::new(
            4.into(),
            2.into(),
            vec![0x12, 0xef],
        ))],
        RecordType::NAPTR => vec![RData::NAPTR(NAPTR::new(
            100,
            10,
            b"S".to_vec().into_boxed_slice(),
            b"SIP+D2U".to_vec().into_boxed_slice(),
            Vec::new().into_boxed_slice(),
            host("_sip._udp.example.com."),
        ))],
        RecordType::HINFO => vec![RData::HINFO(HINFO::new(
            "INTEL".to_string(),
            "LINUX".to_string(),
        ))],
        _ => Vec::new(),
    }
}

fn is_partial(name: &str) -> bool {
    name.trim_end_matches('.')
        .eq_ignore_ascii_case(PARTIAL_NAME)
}

fn servfails(name: &str, record_type: &str) -> bool {
    name.trim_end_matches('.')
        .eq_ignore_ascii_case(SERVFAIL_NAME)
        || (is_partial(name) && record_type == "AAAA")
}

struct ZoneLookup;

impl ResolverLookup for ZoneLookup {
    fn lookup(
        &self,
        resolver: &mhost::resolver::Resolver,
        query: mhost::resolver::MultiQuery,
    ) -> futures::future::BoxFuture<'static, mhost::resolver::ResolverResult<Lookups>> {
        let name_server = resolver.name();
        let lookups: Vec<serde_json::Value> = query
            .into_uni_queries()
            .into_iter()
            .map(|uni| {
                let name = uni.name().to_string();
                let record_type = uni.record_type().to_string();
                let data = if in_zone(&name) || (is_partial(&name) && record_type == "A") {
                    zone_data(&record_type)
                } else if named(&name, TYPES_NAME) {
                    types_data(&record_type)
                } else if named(&name, LAME_NAME) && record_type == "NS" {
                    zone_data("NS")
                } else if named(&name, DIVERGE_NAME) && record_type == "A" {
                    let address = if name_server.contains("8.8.8.8") {
                        "192.0.2.10"
                    } else {
                        "192.0.2.20"
                    };
                    vec![serde_json::json!({ "A": address })]
                } else if named(&name, SOA_NAME) && record_type == "SOA" {
                    vec![serde_json::json!({ "SOA": {
                        "mname": "ns1.example.com.", "rname": "hostmaster.example.com.",
                        "serial": 2_026_101_501u32, "refresh": 7200, "retry": 3600,
                        "expire": 1_209_600, "minimum": 300
                    }})]
                } else {
                    Vec::new()
                };
                let result = if servfails(&name, &record_type) {
                    serde_json::json!({ "NxDomain": {
                        "response_time": { "secs": 0, "nanos": 10_000_000 },
                        "response_code": "SERVFAIL"
                    }})
                } else if data.is_empty() {
                    serde_json::json!({ "NxDomain": {
                        "response_time": { "secs": 0, "nanos": 10_000_000 },
                        "response_code": "NOERROR"
                    }})
                } else {
                    let records: Vec<serde_json::Value> = data
                        .into_iter()
                        .map(|data| {
                            serde_json::json!({
                                "name": name, "type": record_type, "ttl": ZONE_TTL, "data": data
                            })
                        })
                        .collect();
                    serde_json::json!({ "Response": {
                        "records": records,
                        "response_time": { "secs": 0, "nanos": 10_000_000 },
                        "valid_until": "2099-01-01T00:00:00Z"
                    }})
                };
                serde_json::json!({
                    "query": { "name": name, "record_type": record_type },
                    "name_server": name_server,
                    "result": result
                })
            })
            .collect();
        let decoded: Lookups = serde_json::from_value(serde_json::json!({ "lookups": lookups }))
            .expect("zone lookups decode");
        Box::pin(async move { Ok(decoded) })
    }
}

fn zone_data(record_type: &str) -> Vec<serde_json::Value> {
    match record_type {
        "A" => vec![serde_json::json!({ "A": "192.0.2.10" })],
        "MX" => {
            vec![serde_json::json!({ "MX": { "preference": 10, "exchange": "mx1.example.com." } })]
        }
        "TXT" => vec![serde_json::json!({ "TXT": { "txt_data": [b"v=spf1 -all".to_vec()] } })],
        "NS" => vec![
            serde_json::json!({ "NS": "ns1.example.com." }),
            serde_json::json!({ "NS": "ns2.example.com." }),
        ],
        "CAA" => vec![serde_json::json!({
            "CAA": { "issuer_critical": false, "tag": "issue", "value": "letsencrypt.org" }
        })],
        _ => Vec::new(),
    }
}

fn zone_records(name: &Name, record_type: RecordType) -> Vec<Record> {
    if named(&name.to_ascii(), TYPES_NAME) {
        return types_records(record_type)
            .into_iter()
            .map(|data| Record::from_rdata(name.clone(), ZONE_TTL, data))
            .collect();
    }
    if !in_zone(&name.to_ascii()) {
        return Vec::new();
    }
    let host = |h: &str| Name::from_ascii(h).expect("zone host name");
    let rdata = match record_type {
        RecordType::A => vec![RData::A(A(Ipv4Addr::new(192, 0, 2, 10)))],
        RecordType::MX => vec![RData::MX(MX::new(10, host("mx1.example.com.")))],
        RecordType::TXT => vec![RData::TXT(TXT::new(vec!["v=spf1 -all".to_string()]))],
        RecordType::NS => vec![
            RData::NS(NS(host("ns1.example.com."))),
            RData::NS(NS(host("ns2.example.com."))),
        ],
        RecordType::CAA => vec![RData::CAA(CAA::new_issue(
            false,
            Some(host("letsencrypt.org")),
            Vec::new(),
        ))],
        _ => Vec::new(),
    };
    rdata
        .into_iter()
        .map(|data| Record::from_rdata(name.clone(), ZONE_TTL, data))
        .collect()
}

struct ZoneSend;

impl RawSend for ZoneSend {
    fn send(
        &self,
        _server: SocketAddr,
        name: Name,
        record_type: RecordType,
        _dnssec_ok: bool,
        _timeout: Duration,
    ) -> SendFuture {
        let mut message = Message::new(0, MessageType::Response, OpCode::Query);
        message.metadata.authoritative = true;
        if named(&name.to_ascii(), LAME_NAME) {
            message.metadata.authoritative = false;
            message.metadata.response_code = ResponseCode::Refused;
        }
        for record in zone_records(&name, record_type) {
            message.add_answer(record);
        }
        let response = RawResponse {
            message,
            latency: Duration::from_millis(1),
        };
        Box::pin(async move { Ok(response) })
    }
}

/// A backend serving a fixed `example.com.` zone with no network: A 192.0.2.10, MX, TXT, NS
/// (`ns1`, `ns2`) and CAA; every lookup of `dnssec-failed.org` is SERVFAIL; `partial.example.net`
/// answers A (192.0.2.10) and SERVFAILs AAAA; `types.example.com` answers SRV, TLSA, SSHFP, NAPTR
/// and HINFO; `lame.example.com` has the zone's NS, which answer REFUSED; `diverge.example.com`
/// answers A 192.0.2.10 from the server at 8.8.8.8 and 192.0.2.20 from any other;
/// `soa.example.com` answers an SOA with serial 2026101501; every other name and
/// type is NODATA. The raw outbound answers every
/// server authoritatively from the same zone, unsigned.
pub fn stub_backend() -> DnsBackend {
    DnsBackend {
        lookup: Arc::new(ZoneLookup),
        raw: RawOutbound {
            allow: |ip| match ip {
                IpAddr::V4(v4) => !v4.is_private() && !v4.is_loopback(),
                IpAddr::V6(v6) => !v6.is_loopback(),
            },
            resolver: Arc::new(StubResolve(HashMap::from([
                (
                    "ns1.example.com".to_string(),
                    IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
                ),
                (
                    "ns2.example.com".to_string(),
                    IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)),
                ),
            ]))),
            sender: Arc::new(ZoneSend),
        },
    }
}

/// An `AppState` over the default configuration whose backend is [`stub_backend`].
pub fn stub_state() -> AppState {
    let config = crate::config::Config::load(None).expect("default config must be valid");
    AppState {
        circuit_breakers: Arc::new(crate::circuit_breaker::CircuitBreakerRegistry::new(
            &config.circuit_breaker,
        )),
        ip_extractor: Arc::new(crate::security::IpExtractor::new(
            &config.server.trusted_proxies,
        )),
        result_cache: Arc::new(crate::result_cache::ResultCache::new()),
        hot_state: crate::reload::HotState::new(&config),
        ip_enrichment: None,
        query_semaphore: Arc::new(tokio::sync::Semaphore::new(QUERY_SEMAPHORE_PERMITS)),
        config: Arc::new(config),
        backend: stub_backend(),
    }
}

/// The jobs over the default configuration, with system resolvers off as in production, and
/// [`stub_backend`]: fresh circuit breakers, the query semaphore, no enrichment, no `prism_*`
/// metrics.
pub fn stub_jobs() -> DnsJobs {
    let config: ModuleConfig =
        serde_json::from_str(r#"{"dns": {"allow_system_resolvers": false}}"#)
            .expect("default module config");
    stub_jobs_with(
        config
            .into_config()
            .expect("default module config is valid"),
    )
}

/// [`stub_jobs`] over `config`.
pub fn stub_jobs_with(config: Config) -> DnsJobs {
    DnsJobs::new(
        Arc::new(config.clone()),
        Arc::new(crate::circuit_breaker::CircuitBreakerRegistry::new(
            &config.circuit_breaker,
        )),
        Arc::new(tokio::sync::Semaphore::new(QUERY_SEMAPHORE_PERMITS)),
        None,
        stub_backend(),
    )
}
