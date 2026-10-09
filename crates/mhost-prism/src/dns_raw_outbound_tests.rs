//! The outbound policy for raw DNS queries (requirement 1 of
//! `specs/features/raw-query-policy/spec.md`), and the shared test support the other
//! outbound tests use: a recording sender that answers canned responses, a stub glue
//! resolver and a test `allow` that admits documentation addresses as stand-ins for public
//! ones. Nothing here touches the network.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hickory_proto::op::{Message, MessageType, OpCode};
use hickory_proto::rr::rdata::{A, AAAA, NS, SOA};
use hickory_proto::rr::{Name, RData, Record, RecordType};
use netray_common::fetch::Resolve;

use super::{RawError, RawOutbound, RawResponse, RawSend, SendFuture};

// ---------------------------------------------------------------------------
// Shared test support
// ---------------------------------------------------------------------------

/// One query the recording sender saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sent {
    pub(crate) server: SocketAddr,
    pub(crate) name: Name,
    pub(crate) record_type: RecordType,
    pub(crate) dnssec_ok: bool,
}

type Answer = dyn Fn(SocketAddr, &Name, RecordType) -> Option<Message> + Send + Sync;

/// A [`RawSend`] that records every query and answers from a canned function.
/// `None` from the function is a timeout.
pub(crate) struct RecordingSender {
    answer: Box<Answer>,
    sent: Mutex<Vec<Sent>>,
}

impl RecordingSender {
    pub(crate) fn new(
        answer: impl Fn(SocketAddr, &Name, RecordType) -> Option<Message> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            answer: Box::new(answer),
            sent: Mutex::new(Vec::new()),
        })
    }

    /// Every query sent, in send order.
    pub(crate) fn sent(&self) -> Vec<Sent> {
        self.sent.lock().unwrap().clone()
    }

    /// The server address of every query sent.
    pub(crate) fn recorded_ips(&self) -> Vec<IpAddr> {
        self.sent().iter().map(|s| s.server.ip()).collect()
    }

    /// The server addresses that received a query for `name`.
    pub(crate) fn recorded_ips_for(&self, name: &str) -> Vec<IpAddr> {
        let name = Name::from_ascii(name).unwrap();
        self.sent()
            .iter()
            .filter(|s| s.name == name)
            .map(|s| s.server.ip())
            .collect()
    }
}

impl RawSend for RecordingSender {
    fn send(
        &self,
        server: SocketAddr,
        name: Name,
        record_type: RecordType,
        dnssec_ok: bool,
        timeout: Duration,
    ) -> SendFuture {
        self.sent.lock().unwrap().push(Sent {
            server,
            name: name.clone(),
            record_type,
            dnssec_ok,
        });
        let result = match (self.answer)(server, &name, record_type) {
            Some(message) => Ok(RawResponse {
                message,
                latency: Duration::from_millis(1),
            }),
            None => Err(RawError::Timeout(timeout)),
        };
        Box::pin(async move { result })
    }
}

/// A glue resolver answering from a fixed map; it records every host it was asked for.
/// Hosts are looked up as the context passes them (no trailing dot).
pub(crate) struct StubResolver {
    map: HashMap<String, Vec<IpAddr>>,
    asked: Mutex<Vec<String>>,
}

impl StubResolver {
    pub(crate) fn new(entries: &[(&str, &[IpAddr])]) -> Arc<Self> {
        Arc::new(Self {
            map: entries
                .iter()
                .map(|(host, ips)| (host.trim_end_matches('.').to_string(), ips.to_vec()))
                .collect(),
            asked: Mutex::new(Vec::new()),
        })
    }

    /// Every host the context asked for, in order.
    pub(crate) fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Resolve for StubResolver {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let host = host.trim_end_matches('.').to_string();
        self.asked.lock().unwrap().push(host.clone());
        let ips = self.map.get(&host).cloned().unwrap_or_default();
        Box::pin(async move { ips })
    }
}

/// Test `allow`: only `192.0.2.0/24` stands in for a public address.
pub(crate) fn documentation_only(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v4) if v4.octets()[..3] == [192, 0, 2])
}

/// A context with the test `allow`, the given sender and the given resolver.
pub(crate) fn test_outbound(
    sender: Arc<RecordingSender>,
    resolver: Arc<StubResolver>,
) -> RawOutbound {
    RawOutbound {
        allow: documentation_only,
        resolver,
        sender,
    }
}

pub(crate) fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

pub(crate) fn v6(s: &str) -> IpAddr {
    IpAddr::V6(s.parse::<Ipv6Addr>().unwrap())
}

pub(crate) fn name(s: &str) -> Name {
    Name::from_ascii(s).unwrap()
}

/// An empty NOERROR response; `authoritative` sets AA.
pub(crate) fn reply(authoritative: bool) -> Message {
    let mut m = Message::new(0, MessageType::Response, OpCode::Query);
    m.metadata.authoritative = authoritative;
    m
}

pub(crate) fn ns_record(owner: &str, target: &str) -> Record {
    Record::from_rdata(name(owner), 3600, RData::NS(NS(name(target))))
}

pub(crate) fn a_record(owner: &str, ip: Ipv4Addr) -> Record {
    Record::from_rdata(name(owner), 3600, RData::A(A(ip)))
}

pub(crate) fn aaaa_record(owner: &str, ip: Ipv6Addr) -> Record {
    Record::from_rdata(name(owner), 3600, RData::AAAA(AAAA(ip)))
}

pub(crate) fn soa_record(owner: &str) -> Record {
    Record::from_rdata(
        name(owner),
        3600,
        RData::SOA(SOA::new(
            name(&format!("ns1.{owner}")),
            name(&format!("hostmaster.{owner}")),
            1,
            7200,
            3600,
            1_209_600,
            300,
        )),
    )
}

/// A referral (AA=0) to `zone` with the given NS names in authority and glue in additional.
pub(crate) fn referral(zone: &str, ns: &[&str], glue: &[(&str, IpAddr)]) -> Message {
    let mut m = reply(false);
    for target in ns {
        m.add_authority(ns_record(zone, target));
    }
    for (owner, ip) in glue {
        match ip {
            IpAddr::V4(v4) => m.add_additional(a_record(owner, *v4)),
            IpAddr::V6(v6) => m.add_additional(aaaa_record(owner, *v6)),
        };
    }
    m
}

/// True if `ip` is one of the root servers.
pub(crate) fn is_root(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v4) if super::ROOT_SERVERS.contains(&v4))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

fn ns_map(entries: &[(&str, &[IpAddr])]) -> HashMap<String, Vec<IpAddr>> {
    entries
        .iter()
        .map(|(host, ips)| (host.to_string(), ips.to_vec()))
        .collect()
}

fn idle_outbound(resolver: Arc<StubResolver>) -> (RawOutbound, Arc<RecordingSender>) {
    let sender = RecordingSender::new(|_, _, _| Some(reply(true)));
    (test_outbound(sender.clone(), resolver), sender)
}

#[test]
fn build_server_list_keeps_allowed_ipv4_with_port_53_and_refuses_none() {
    let (raw, _) = idle_outbound(StubResolver::new(&[]));
    let map = ns_map(&[
        ("ns1.example.com.", &[v4(192, 0, 2, 53)]),
        ("ns2.example.com.", &[v4(192, 0, 2, 54), v4(192, 0, 2, 55)]),
    ]);
    let list = raw.build_server_list(&map);
    let mut got: Vec<SocketAddr> = list.servers.iter().map(|(a, _)| *a).collect();
    got.sort();
    let want: Vec<SocketAddr> = [53u8, 54, 55]
        .iter()
        .map(|d| SocketAddr::new(v4(192, 0, 2, *d), 53))
        .collect();
    assert_eq!(got, want);
    assert_eq!(list.refused, 0);
}

#[test]
fn build_server_list_refuses_not_public_ipv4_counts_them_and_drops_ipv6_uncounted() {
    let (raw, _) = idle_outbound(StubResolver::new(&[]));
    let map = ns_map(&[(
        "ns1.example.com.",
        &[
            v4(192, 0, 2, 53),
            v4(10, 0, 0, 1),
            v4(172, 16, 0, 5),
            v4(127, 0, 0, 1),
            v6("2001:db8::53"),
        ],
    )]);
    let list = raw.build_server_list(&map);
    let got: Vec<SocketAddr> = list.servers.iter().map(|(a, _)| *a).collect();
    assert_eq!(got, vec![SocketAddr::new(v4(192, 0, 2, 53), 53)]);
    assert_eq!(list.refused, 3);
}

#[tokio::test]
async fn resolve_missing_glue_does_not_resolve_a_name_whose_glue_is_all_refused() {
    let resolver = StubResolver::new(&[("ns1.example.com", &[v4(192, 0, 2, 53)])]);
    let (raw, _) = idle_outbound(resolver.clone());
    let mut map = ns_map(&[("ns1.example.com.", &[v4(10, 0, 0, 1)])]);
    raw.resolve_missing_glue(&mut map).await;
    assert!(resolver.asked().is_empty(), "asked: {:?}", resolver.asked());
    let list = raw.build_server_list(&map);
    assert!(list.servers.is_empty());
    assert_eq!(list.refused, 1);
}

#[tokio::test]
async fn resolve_missing_glue_resolves_empty_names_without_trailing_dot_and_keeps_ipv4_only() {
    let resolver =
        StubResolver::new(&[("ns2.example.com", &[v4(192, 0, 2, 54), v6("2001:db8::54")])]);
    let (raw, _) = idle_outbound(resolver.clone());
    let mut map = ns_map(&[
        ("ns1.example.com.", &[v4(192, 0, 2, 53)]),
        ("ns2.example.com.", &[]),
    ]);
    raw.resolve_missing_glue(&mut map).await;
    assert_eq!(resolver.asked(), vec!["ns2.example.com".to_string()]);
    assert_eq!(map["ns2.example.com."], vec![v4(192, 0, 2, 54)]);
    assert_eq!(map["ns1.example.com."], vec![v4(192, 0, 2, 53)]);
}

#[tokio::test]
async fn resolved_not_public_addresses_are_refused_and_counted_not_listed() {
    let resolver = StubResolver::new(&[("ns2.example.com", &[v4(10, 0, 0, 1), v4(192, 0, 2, 54)])]);
    let (raw, _) = idle_outbound(resolver);
    let mut map = ns_map(&[("ns2.example.com.", &[])]);
    raw.resolve_missing_glue(&mut map).await;
    let list = raw.build_server_list(&map);
    let got: Vec<SocketAddr> = list.servers.iter().map(|(a, _)| *a).collect();
    assert_eq!(got, vec![SocketAddr::new(v4(192, 0, 2, 54), 53)]);
    assert_eq!(list.refused, 1);
}

#[tokio::test]
async fn queries_go_through_the_sender_with_the_dnssec_flag_of_the_call() {
    let (raw, sender) = idle_outbound(StubResolver::new(&[]));
    let server = SocketAddr::new(v4(192, 0, 2, 53), 53);
    let n = name("example.com.");
    let timeout = Duration::from_secs(1);

    let results = raw
        .parallel_queries(&[server], &n, RecordType::NS, timeout)
        .await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].server, server);
    assert!(results[0].result.is_ok());

    let response = raw
        .raw_query_dnssec(server, &n, RecordType::DNSKEY, timeout)
        .await;
    assert!(response.is_ok());

    let sent = sender.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(
        (sent[0].record_type, sent[0].dnssec_ok),
        (RecordType::NS, false)
    );
    assert_eq!(
        (sent[1].record_type, sent[1].dnssec_ok),
        (RecordType::DNSKEY, true)
    );
    assert!(sent.iter().all(|s| s.server == server && s.name == n));
}

#[tokio::test]
async fn queries_to_a_list_built_by_the_context_never_reach_a_refused_address() {
    let (raw, sender) = idle_outbound(StubResolver::new(&[]));
    let map = ns_map(&[
        ("ns1.example.com.", &[v4(192, 0, 2, 53), v4(10, 0, 0, 1)]),
        ("ns2.example.com.", &[v4(172, 16, 0, 5), v4(127, 0, 0, 1)]),
    ]);
    let list = raw.build_server_list(&map);
    let servers: Vec<SocketAddr> = list.servers.iter().map(|(a, _)| *a).collect();
    raw.parallel_queries(
        &servers,
        &name("example.com."),
        RecordType::NS,
        Duration::from_secs(1),
    )
    .await;
    assert_eq!(sender.recorded_ips(), vec![v4(192, 0, 2, 53)]);
}

#[test]
fn production_context_allows_exactly_what_is_allowed_target_allows() {
    use netray_common::target_policy::is_allowed_target;
    let ctx = RawOutbound::production();
    assert!(std::ptr::fn_addr_eq(
        ctx.allow,
        is_allowed_target as fn(IpAddr) -> bool
    ));
    for ip in [
        v4(10, 0, 0, 1),
        v4(172, 16, 0, 5),
        v4(127, 0, 0, 1),
        v4(192, 0, 2, 53),
        v4(198, 51, 100, 1),
    ] {
        assert_eq!((ctx.allow)(ip), is_allowed_target(ip), "{ip}");
    }
}
