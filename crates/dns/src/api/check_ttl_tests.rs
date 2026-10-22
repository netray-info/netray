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
