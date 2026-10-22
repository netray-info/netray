//! `dns.ttl` judges a low recursive TTL on the TTL the zone's nameservers serve (requirement 7
//! of `specs/features/v2-correctness/spec.md`).
//!
//! Nothing here touches the network. The recursive resolvers (198.51.100.1 and .2) and the
//! zone's nameservers (192.0.2.53 and .54, glue for ns1 and ns2.example.com) answer from one
//! stub. The authoritative TTL is served on both seams: through the raw sender (an AA answer
//! to a query at a nameserver address) and through the lookup seam (a resolver whose name
//! holds a nameserver address), so an implementation may read it either way.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

use hickory_proto::rr::rdata::A;
use hickory_proto::rr::{Name, RData, Record, RecordType};
use mhost::nameserver::NameServerConfig;
use mhost::resolver::{Lookups, MultiQuery, Resolver, ResolverGroupBuilder, ResolverResult};
use netray_common::fetch::ClientSettings;
use netray_engine::SectionOutcome;
use netray_model::evidence::{Shape, Value};
use netray_model::{CheckId, Status};
use serde_json::{Value as Json, json};
use tokio::sync::{Semaphore, mpsc};

use super::{CheckEvent, CheckRun, Outbound, run_check_on};
use crate::circuit_breaker::CircuitBreakerRegistry;
use crate::dns_raw::outbound_tests::{
    RecordingSender, StubResolver, ns_record, reply, soa_record, test_outbound, v4,
};
use crate::jobs::{DnsBackend, ResolverLookup};
use crate::module::{block_id, translate};

const NS_ADDRS: [&str; 2] = ["192.0.2.53", "192.0.2.54"];

/// The TTLs a scenario serves for `example.com A 192.0.2.10`.
#[derive(Clone, Copy)]
struct Scenario {
    recursive: u32,
    /// `None`: no authoritative server answers.
    authoritative: Option<u32>,
}

/// Every (resolver name, query name, type) the lookup seam was asked.
#[derive(Default)]
struct Asked(Mutex<Vec<(String, String, String)>>);

struct TtlLookup {
    scenario: Scenario,
    asked: Arc<Asked>,
}

impl ResolverLookup for TtlLookup {
    fn lookup(
        &self,
        resolver: &Resolver,
        query: MultiQuery,
    ) -> futures::future::BoxFuture<'static, ResolverResult<Lookups>> {
        let name_server = resolver.name();
        let at_nameserver = NS_ADDRS.iter().any(|a| name_server.contains(a));
        let scenario = self.scenario;
        let lookups: Vec<Json> = query
            .into_uni_queries()
            .into_iter()
            .map(|uni| {
                let name = uni.name().to_ascii();
                let rtype = uni.record_type().to_string();
                self.asked.0.lock().unwrap().push((
                    name_server.clone(),
                    name.trim_end_matches('.').to_ascii_lowercase(),
                    rtype.clone(),
                ));
                let time = json!({ "secs": 0, "nanos": 10_000_000 });
                let apex = name
                    .trim_end_matches('.')
                    .eq_ignore_ascii_case("example.com");
                let served = |ttl: u32, data: Json| {
                    json!({ "Response": {
                        "records": [{ "name": name, "type": rtype, "ttl": ttl, "data": data }],
                        "response_time": time,
                        "valid_until": "2099-01-01T00:00:00Z"
                    }})
                };
                let result = match (apex, rtype.as_str(), at_nameserver) {
                    (true, "SOA", false) => served(300, soa()),
                    (true, "NS", false) => served(300, json!({ "NS": "ns1.example.com." })),
                    (true, "A", false) => served(scenario.recursive, json!({ "A": "192.0.2.10" })),
                    (true, "A", true) => match scenario.authoritative {
                        Some(ttl) => served(ttl, json!({ "A": "192.0.2.10" })),
                        None => json!({ "NxDomain": {
                            "response_time": time, "response_code": "SERVFAIL"
                        }}),
                    },
                    _ => json!({ "NxDomain": {
                        "response_time": time, "response_code": "NOERROR"
                    }}),
                };
                json!({
                    "query": { "name": name, "record_type": rtype },
                    "name_server": name_server,
                    "result": result
                })
            })
            .collect();
        let decoded: Lookups =
            serde_json::from_value(json!({ "lookups": lookups })).expect("ttl lookups decode");
        Box::pin(async move { Ok(decoded) })
    }
}

fn soa() -> Json {
    json!({ "SOA": {
        "mname": "ns1.example.net.", "rname": "hostmaster.example.net.",
        "serial": 1, "refresh": 7200, "retry": 3600, "expire": 1_209_600, "minimum": 300
    }})
}

struct Ran {
    outcome: SectionOutcome,
    asked: Arc<Asked>,
    sender: Arc<RecordingSender>,
}

async fn run(scenario: Scenario) -> Ran {
    let group = ResolverGroupBuilder::new()
        .nameserver(NameServerConfig::udp(
            "198.51.100.1:53".parse::<SocketAddr>().unwrap(),
        ))
        .nameserver(NameServerConfig::udp(
            "198.51.100.2:53".parse::<SocketAddr>().unwrap(),
        ))
        .build()
        .await
        .expect("resolver group over documentation addresses");
    let config = crate::config::Config::load(None).expect("default config");
    let sender = RecordingSender::new(move |_: SocketAddr, name: &Name, rtype: RecordType| {
        let owner = name.to_ascii();
        let mut m = reply(true);
        match rtype {
            RecordType::SOA => {
                m.add_answer(soa_record(&owner));
            }
            RecordType::NS => {
                m.add_answer(ns_record(&owner, "ns1.example.com."));
                m.add_answer(ns_record(&owner, "ns2.example.com."));
            }
            RecordType::A if owner.eq_ignore_ascii_case("example.com.") => {
                let ttl = scenario.authoritative?;
                m.add_answer(Record::from_rdata(
                    name.clone(),
                    ttl,
                    RData::A(A("192.0.2.10".parse().unwrap())),
                ));
            }
            _ => return None,
        }
        Some(m)
    });
    let (ip1, ip2) = ([v4(192, 0, 2, 53)], [v4(192, 0, 2, 54)]);
    let glue_table: Vec<(&str, &[IpAddr])> =
        vec![("ns1.example.com", &ip1), ("ns2.example.com", &ip2)];
    let asked = Arc::new(Asked::default());
    let backend = DnsBackend {
        lookup: Arc::new(TtlLookup {
            scenario,
            asked: Arc::clone(&asked),
        }),
        raw: test_outbound(sender.clone(), StubResolver::new(&glue_table)),
    };
    let outbound = Outbound {
        settings: ClientSettings::default(),
        resolver: StubResolver::new(&[]),
        allow: |_| false,
    };
    let run = CheckRun {
        request_id: "ttl-test".to_owned(),
        domain: "example.com".to_owned(),
        resolvers: group.resolvers().to_vec(),
        breaker_keys: vec!["one".to_owned(), "two".to_owned()],
        circuit_breakers: Arc::new(CircuitBreakerRegistry::new(&config.circuit_breaker)),
        query_semaphore: Arc::new(Semaphore::new(8)),
        ip_enrichment: None,
    };
    let (tx, mut rx) = mpsc::channel::<CheckEvent>(64);
    let collect = async move {
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    };
    let ((), events) = tokio::join!(run_check_on(run, tx, backend, outbound), collect);
    Ran {
        outcome: translate(&events),
        asked,
        sender,
    }
}

impl Ran {
    fn ttl_check(&self) -> (Status, Vec<String>) {
        let SectionOutcome::Measured { checks, .. } = &self.outcome else {
            panic!("section not measured: {:?}", self.outcome);
        };
        let id = CheckId::parse("dns.ttl").unwrap();
        let found = checks
            .iter()
            .find(|c| c.id == id)
            .expect("no check dns.ttl");
        (found.status.clone(), found.findings.clone())
    }

    /// The TTLs of the records of block `dns.a`.
    fn a_block_ttls(&self) -> Vec<i64> {
        let SectionOutcome::Measured { evidence, .. } = &self.outcome else {
            panic!("section not measured: {:?}", self.outcome);
        };
        let id = block_id("A");
        let block = evidence
            .iter()
            .find(|b| b.id == id)
            .expect("no block dns.a");
        let Shape::Records(records) = &block.shape else {
            panic!("block dns.a is not records");
        };
        records
            .iter()
            .map(|r| match &r.ttl {
                Value::Int(ttl) => *ttl,
                other => panic!("ttl is not an int: {other:?}"),
            })
            .collect()
    }

    /// Whether an A query for example.com reached a nameserver address, on either seam.
    fn asked_authoritative_a(&self) -> bool {
        let raw = self.sender.sent().iter().any(|s| {
            s.record_type == RecordType::A
                && s.name.to_ascii().eq_ignore_ascii_case("example.com.")
                && NS_ADDRS.contains(&s.server.ip().to_string().as_str())
        });
        let seam = self
            .asked
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|(server, name, rtype)| {
                rtype == "A" && name == "example.com" && NS_ADDRS.iter().any(|a| server.contains(a))
            });
        raw || seam
    }
}

/// C2 (and C1): resolvers 29 s, authoritative 60 s is not low; the block shows 60.
#[tokio::test]
async fn c2_authoritative_ttl_of_60_is_not_low_and_the_block_shows_it() {
    let ran = run(Scenario {
        recursive: 29,
        authoritative: Some(60),
    })
    .await;

    let (status, findings) = ran.ttl_check();
    assert_eq!(status, Status::Pass, "{findings:?}");
    assert_eq!(ran.a_block_ttls(), vec![60]);
}

/// C3 (and C1): resolvers 29 s, authoritative 30 s is low and the finding says 30s.
#[tokio::test]
async fn c3_authoritative_ttl_below_60_warns_with_that_ttl() {
    let ran = run(Scenario {
        recursive: 29,
        authoritative: Some(30),
    })
    .await;

    let (status, findings) = ran.ttl_check();
    assert_eq!(status, Status::Warn, "{findings:?}");
    assert!(
        findings
            .iter()
            .any(|f| f.contains("example.com") && f.contains("30s")),
        "finding must name example.com. with 30s: {findings:?}"
    );
}

/// C4 (and C1): no authoritative answer keeps the recursive 29 s.
#[tokio::test]
async fn c4_no_authoritative_answer_keeps_the_recursive_ttl() {
    let ran = run(Scenario {
        recursive: 29,
        authoritative: None,
    })
    .await;

    let (status, findings) = ran.ttl_check();
    assert_eq!(status, Status::Warn, "{findings:?}");
    assert!(
        findings.iter().any(|f| f.contains("29s")),
        "finding must carry 29s: {findings:?}"
    );
}

/// C5 (and C1): a recursive TTL of 60 s or more makes no authoritative A lookup.
#[tokio::test]
async fn c5_recursive_ttl_of_300_makes_no_authoritative_lookup() {
    let ran = run(Scenario {
        recursive: 300,
        authoritative: Some(60),
    })
    .await;

    assert_eq!(ran.ttl_check().0, Status::Pass);
    assert!(
        !ran.asked_authoritative_a(),
        "no A query for example.com may reach a nameserver address"
    );
}

// ---------------------------------------------------------------------------------------
// Phase 3, C6 to C9: scenarios over a configurable zone, names and record sets.
// ---------------------------------------------------------------------------------------

/// The data of a served record.
#[derive(Clone, Copy)]
enum Rd {
    A(&'static str),
    Cname(&'static str),
    /// A signature over the apex A set; served by the recursive stub only.
    Rrsig,
}

impl Rd {
    fn json(self) -> (&'static str, Json) {
        match self {
            Rd::A(ip) => ("A", json!({ "A": ip })),
            Rd::Cname(target) => ("CNAME", json!({ "CNAME": target })),
            Rd::Rrsig => (
                "RRSIG",
                json!({ "RRSIG": {
                    "type_covered": "A", "algorithm": "EcdsaP256Sha256", "labels": 2,
                    "original_ttl": 29, "expiration": 4_102_444_800u64,
                    "inception": 1_700_000_000u64, "key_tag": 2371,
                    "signer_name": "example.com.", "signature": "c2ln"
                }}),
            ),
        }
    }

    fn rdata(self) -> RData {
        match self {
            Rd::A(ip) => RData::A(A(ip.parse().unwrap())),
            Rd::Cname(target) => RData::CNAME(hickory_proto::rr::rdata::CNAME(
                Name::from_ascii(target).unwrap(),
            )),
            Rd::Rrsig => unreachable!("an RRSIG is served by the recursive stub only"),
        }
    }
}

/// A record in an answer; `owner` is the A-label name with its trailing dot.
#[derive(Clone, Copy)]
struct Rr {
    owner: &'static str,
    ttl: u32,
    rd: Rd,
}

/// The records served for one question. `at` limits a recursive answer to the resolver whose
/// name contains it.
struct Answer {
    at: Option<&'static str>,
    qname: &'static str,
    qtype: RecordType,
    records: Vec<Rr>,
}

fn answer(qname: &'static str, qtype: RecordType, records: Vec<Rr>) -> Answer {
    Answer {
        at: None,
        qname,
        qtype,
        records,
    }
}

struct World {
    domain: &'static str,
    zone: &'static str,
    /// (nameserver host, glue address)
    ns: Vec<(&'static str, &'static str)>,
    recursive: Vec<Answer>,
    authoritative: Vec<Answer>,
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim_end_matches('.')
        .eq_ignore_ascii_case(b.trim_end_matches('.'))
}

fn find<'a>(
    list: &'a [Answer],
    resolver: Option<&str>,
    qname: &str,
    qtype: &str,
) -> Option<&'a Answer> {
    list.iter().find(|a| {
        a.qtype.to_string() == qtype
            && same_name(a.qname, qname)
            && a.at
                .is_none_or(|at| resolver.is_some_and(|name| name.contains(at)))
    })
}

struct WorldLookup {
    world: Arc<World>,
    asked: Arc<Asked>,
}

impl ResolverLookup for WorldLookup {
    fn lookup(
        &self,
        resolver: &Resolver,
        query: MultiQuery,
    ) -> futures::future::BoxFuture<'static, ResolverResult<Lookups>> {
        let world = &self.world;
        let name_server = resolver.name();
        let at_nameserver = world.ns.iter().any(|(_, ip)| name_server.contains(ip));
        let lookups: Vec<Json> = query
            .into_uni_queries()
            .into_iter()
            .map(|uni| {
                let name = uni.name().to_ascii();
                let rtype = uni.record_type().to_string();
                self.asked.0.lock().unwrap().push((
                    name_server.clone(),
                    name.trim_end_matches('.').to_ascii_lowercase(),
                    rtype.clone(),
                ));
                let time = json!({ "secs": 0, "nanos": 10_000_000 });
                let table = if at_nameserver {
                    &world.authoritative
                } else {
                    &world.recursive
                };
                let owner_json = |owner: &str, ttl: u32, rd: Rd| {
                    let (kind, data) = rd.json();
                    json!({ "name": owner, "type": kind, "ttl": ttl, "data": data })
                };
                let mut records: Vec<Json> = Vec::new();
                if let Some(found) = find(table, Some(&name_server), &name, &rtype) {
                    records.extend(
                        found
                            .records
                            .iter()
                            .map(|r| owner_json(r.owner, r.ttl, r.rd)),
                    );
                } else if !at_nameserver && same_name(&name, world.zone) {
                    match rtype.as_str() {
                        "SOA" => {
                            records.push(
                                json!({ "name": name, "type": "SOA", "ttl": 300, "data": soa() }),
                            );
                        }
                        "NS" => records.extend(world.ns.iter().map(|(host, _)| {
                            json!({ "name": name, "type": "NS", "ttl": 300,
                                    "data": { "NS": format!("{host}.") } })
                        })),
                        _ => {}
                    }
                }
                let result = if records.is_empty() {
                    json!({ "NxDomain": { "response_time": time, "response_code": "NOERROR" }})
                } else {
                    json!({ "Response": {
                        "records": records,
                        "response_time": time,
                        "valid_until": "2099-01-01T00:00:00Z"
                    }})
                };
                json!({
                    "query": { "name": name, "record_type": rtype },
                    "name_server": name_server,
                    "result": result
                })
            })
            .collect();
        let decoded: Lookups =
            serde_json::from_value(json!({ "lookups": lookups })).expect("world lookups decode");
        Box::pin(async move { Ok(decoded) })
    }
}

async fn run_world(world: World) -> Ran {
    let world = Arc::new(world);
    let group = ResolverGroupBuilder::new()
        .nameserver(NameServerConfig::udp(
            "198.51.100.1:53".parse::<SocketAddr>().unwrap(),
        ))
        .nameserver(NameServerConfig::udp(
            "198.51.100.2:53".parse::<SocketAddr>().unwrap(),
        ))
        .build()
        .await
        .expect("resolver group over documentation addresses");
    let config = crate::config::Config::load(None).expect("default config");
    let raw_world = Arc::clone(&world);
    let sender = RecordingSender::new(move |server: SocketAddr, name: &Name, rtype: RecordType| {
        let owner = name.to_ascii();
        let mut m = reply(true);
        match rtype {
            RecordType::SOA => {
                m.add_answer(soa_record(&owner));
            }
            RecordType::NS => {
                for (host, _) in &raw_world.ns {
                    m.add_answer(ns_record(&owner, &format!("{host}.")));
                }
            }
            _ => {
                let found = find(
                    &raw_world.authoritative,
                    Some(&server.ip().to_string()),
                    &owner,
                    &rtype.to_string(),
                )?;
                for r in &found.records {
                    m.add_answer(Record::from_rdata(
                        Name::from_ascii(r.owner).unwrap(),
                        r.ttl,
                        r.rd.rdata(),
                    ));
                }
            }
        }
        Some(m)
    });
    let ips: Vec<[IpAddr; 1]> = world
        .ns
        .iter()
        .map(|(_, ip)| [ip.parse::<IpAddr>().unwrap()])
        .collect();
    let glue_table: Vec<(&str, &[IpAddr])> = world
        .ns
        .iter()
        .zip(&ips)
        .map(|((host, _), ip)| (*host, ip.as_slice()))
        .collect();
    let asked = Arc::new(Asked::default());
    let backend = DnsBackend {
        lookup: Arc::new(WorldLookup {
            world: Arc::clone(&world),
            asked: Arc::clone(&asked),
        }),
        raw: test_outbound(sender.clone(), StubResolver::new(&glue_table)),
    };
    let outbound = Outbound {
        settings: ClientSettings::default(),
        resolver: StubResolver::new(&[]),
        allow: |_| false,
    };
    let run = CheckRun {
        request_id: "ttl-test".to_owned(),
        domain: world.domain.to_owned(),
        resolvers: group.resolvers().to_vec(),
        breaker_keys: vec!["one".to_owned(), "two".to_owned()],
        circuit_breakers: Arc::new(CircuitBreakerRegistry::new(&config.circuit_breaker)),
        query_semaphore: Arc::new(Semaphore::new(8)),
        ip_enrichment: None,
    };
    let (tx, mut rx) = mpsc::channel::<CheckEvent>(64);
    let collect = async move {
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    };
    let ((), events) = tokio::join!(run_check_on(run, tx, backend, outbound), collect);
    Ran {
        outcome: translate(&events),
        asked,
        sender,
    }
}

impl Ran {
    /// Whether any query for `name` reached a nameserver address, on either seam.
    fn asked_nameserver_about(&self, name: &str) -> bool {
        let raw = self.sender.sent().iter().any(|s| {
            same_name(&s.name.to_ascii(), name)
                && NS_ADDRS.contains(&s.server.ip().to_string().as_str())
        });
        let seam = self
            .asked
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|(server, asked, _)| {
                same_name(asked, name) && NS_ADDRS.iter().any(|a| server.contains(a))
            });
        raw || seam
    }
}

const EXAMPLE_NS: [(&str, &str); 2] = [
    ("ns1.example.com", "192.0.2.53"),
    ("ns2.example.com", "192.0.2.54"),
];

/// C6: the TTL belongs to the RRset; a second address the authoritative server does not serve
/// does not make the RRset's 60 s low.
#[tokio::test]
async fn c6_a_record_set_member_at_29_does_not_lower_the_authoritative_60() {
    let apex = |ttl, ip| Rr {
        owner: "example.com.",
        ttl,
        rd: Rd::A(ip),
    };
    let ran = run_world(World {
        domain: "example.com",
        zone: "example.com",
        ns: EXAMPLE_NS.to_vec(),
        recursive: vec![
            Answer {
                at: Some("198.51.100.1"),
                ..answer("example.com", RecordType::A, vec![apex(60, "192.0.2.10")])
            },
            Answer {
                at: Some("198.51.100.2"),
                ..answer("example.com", RecordType::A, vec![apex(29, "192.0.2.11")])
            },
        ],
        authoritative: vec![answer(
            "example.com",
            RecordType::A,
            vec![apex(60, "192.0.2.10")],
        )],
    })
    .await;

    let (status, findings) = ran.ttl_check();
    assert_eq!(status, Status::Pass, "{findings:?}");
}

/// C7: an internationalised apex is looked up at its nameservers under its A-label owner.
#[tokio::test]
async fn c7_idn_apex_reads_its_authoritative_ttl_under_the_a_label() {
    let apex = |ttl| Rr {
        owner: "xn--bcher-kva.example.",
        ttl,
        rd: Rd::A("192.0.2.10"),
    };
    let ran = run_world(World {
        domain: "xn--bcher-kva.example",
        zone: "xn--bcher-kva.example",
        ns: vec![("ns1.xn--bcher-kva.example", "192.0.2.53")],
        recursive: vec![answer(
            "xn--bcher-kva.example",
            RecordType::A,
            vec![apex(29)],
        )],
        authoritative: vec![answer(
            "xn--bcher-kva.example",
            RecordType::A,
            vec![apex(300)],
        )],
    })
    .await;

    let (status, findings) = ran.ttl_check();
    assert_eq!(status, Status::Pass, "{findings:?}");
}

/// C8: a CNAME carried only in the A lookup is read at its owner as a CNAME, not as an A.
#[tokio::test]
async fn c8_cname_only_in_the_a_lookup_reads_its_authoritative_ttl() {
    let cname = |ttl| Rr {
        owner: "www.example.com.",
        ttl,
        rd: Rd::Cname("web.example.com."),
    };
    let web = |ttl| Rr {
        owner: "web.example.com.",
        ttl,
        rd: Rd::A("192.0.2.10"),
    };
    let ran = run_world(World {
        domain: "www.example.com",
        zone: "example.com",
        ns: EXAMPLE_NS.to_vec(),
        recursive: vec![answer(
            "www.example.com",
            RecordType::A,
            vec![cname(29), web(300)],
        )],
        authoritative: vec![
            answer("www.example.com", RecordType::CNAME, vec![cname(300)]),
            answer("web.example.com", RecordType::A, vec![web(300)]),
        ],
    })
    .await;

    let (status, findings) = ran.ttl_check();
    assert_eq!(status, Status::Pass, "{findings:?}");
}

/// C9: a CNAME target outside the zone is never asked of the zone's nameservers.
#[tokio::test]
async fn c9_cname_target_outside_the_zone_is_not_asked_at_the_nameservers() {
    let ran = run_world(World {
        domain: "www.example.com",
        zone: "example.com",
        ns: EXAMPLE_NS.to_vec(),
        recursive: vec![answer(
            "www.example.com",
            RecordType::A,
            vec![
                Rr {
                    owner: "www.example.com.",
                    ttl: 300,
                    rd: Rd::Cname("shop.example.net."),
                },
                Rr {
                    owner: "shop.example.net.",
                    ttl: 20,
                    rd: Rd::A("192.0.2.20"),
                },
            ],
        )],
        authoritative: vec![],
    })
    .await;

    assert!(
        !ran.asked_nameserver_about("shop.example.net"),
        "shop.example.net is outside the zone: no query for it may reach a nameserver"
    );
    let (status, findings) = ran.ttl_check();
    assert_eq!(status, Status::Warn, "{findings:?}");
    assert!(
        findings.iter().any(|f| f.contains("20s")),
        "finding must carry 20s: {findings:?}"
    );
}

/// C10: the RRSIG of a signed apex carries the signature's own TTL; no RRSIG query for the apex
/// reaches a nameserver address, on either seam.
#[tokio::test]
async fn c10_rrsig_is_not_reread_for_ttl() {
    let apex = |ttl, rd| Rr {
        owner: "example.com.",
        ttl,
        rd,
    };
    let ran = run_world(World {
        domain: "example.com",
        zone: "example.com",
        ns: EXAMPLE_NS.to_vec(),
        recursive: vec![
            answer(
                "example.com",
                RecordType::A,
                vec![apex(29, Rd::A("192.0.2.10"))],
            ),
            answer("example.com", RecordType::RRSIG, vec![apex(29, Rd::Rrsig)]),
        ],
        authoritative: vec![answer(
            "example.com",
            RecordType::A,
            vec![apex(60, Rd::A("192.0.2.10"))],
        )],
    })
    .await;

    let raw = ran.sender.sent().iter().any(|s| {
        s.record_type == RecordType::RRSIG
            && same_name(&s.name.to_ascii(), "example.com")
            && NS_ADDRS.contains(&s.server.ip().to_string().as_str())
    });
    let seam = ran
        .asked
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|(server, name, rtype)| {
            rtype == "RRSIG" && name == "example.com" && NS_ADDRS.iter().any(|a| server.contains(a))
        });
    assert!(
        !raw,
        "no RRSIG query for example.com may reach a nameserver (raw seam)"
    );
    assert!(
        !seam,
        "no RRSIG query for example.com may reach a nameserver (lookup seam)"
    );
}
