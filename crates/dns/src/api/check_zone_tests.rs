//! The zone of a name below the apex (requirements 3 to 6 of
//! `specs/features/v2-correctness/spec.md`): the NS, DNSSEC and CAA checks of `run_check_on`
//! judge the zone of the queried name, and CAA climbs to the first ancestor that holds it.
//!
//! Nothing here touches the network: the lookups answer from a canned table that records every
//! (name, type) it was asked, the raw NS queries go through the recording sender, and the
//! MTA-STS fetch refuses every address.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use hickory_proto::op::Message;
use hickory_proto::rr::{Name, RecordType};
use mhost::nameserver::NameServerConfig;
use mhost::resolver::{Lookups, MultiQuery, Resolver, ResolverGroupBuilder, ResolverResult};
use netray_common::fetch::ClientSettings;
use netray_engine::SectionOutcome;
use netray_model::evidence::{Block, Shape, Value};
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

// ---------------------------------------------------------------------------
// The canned lookups
// ---------------------------------------------------------------------------

fn key(name: &str) -> String {
    name.trim_end_matches('.').to_ascii_lowercase()
}

/// What a name and type answer; anything not listed is a definite NODATA.
enum Answer {
    Records(Vec<Json>),
    ServFail,
}

#[derive(Default)]
struct Zones {
    answers: Vec<((String, String), Answer)>,
    asked: Mutex<Vec<(String, String)>>,
}

impl Zones {
    fn with(mut self, name: &str, rtype: &str, data: Vec<Json>) -> Self {
        self.answers
            .push(((key(name), rtype.to_owned()), Answer::Records(data)));
        self
    }

    fn servfail(mut self, name: &str, rtype: &str) -> Self {
        self.answers
            .push(((key(name), rtype.to_owned()), Answer::ServFail));
        self
    }

    /// Every (name, type) asked, once, in sorted order.
    fn asked(&self) -> BTreeSet<(String, String)> {
        self.asked.lock().unwrap().iter().cloned().collect()
    }

    fn was_asked(&self, name: &str, rtype: &str) -> bool {
        self.asked().contains(&(key(name), rtype.to_owned()))
    }

    fn asked_name(&self, name: &str) -> bool {
        self.asked().iter().any(|(n, _)| *n == key(name))
    }
}

struct ZonesLookup(Arc<Zones>);

impl ResolverLookup for ZonesLookup {
    fn lookup(
        &self,
        resolver: &Resolver,
        query: MultiQuery,
    ) -> futures::future::BoxFuture<'static, ResolverResult<Lookups>> {
        let name_server = resolver.name();
        let zones = Arc::clone(&self.0);
        let lookups: Vec<Json> = query
            .into_uni_queries()
            .into_iter()
            .map(|uni| {
                let name = uni.name().to_string();
                let rtype = uni.record_type().to_string();
                zones
                    .asked
                    .lock()
                    .unwrap()
                    .push((key(&name), rtype.clone()));
                let found = zones
                    .answers
                    .iter()
                    .find(|((n, t), _)| *n == key(&name) && *t == rtype)
                    .map(|(_, answer)| answer);
                let time = json!({ "secs": 0, "nanos": 10_000_000 });
                let result = match found {
                    Some(Answer::Records(data)) => {
                        let records: Vec<Json> = data
                            .iter()
                            .map(|data| {
                                json!({ "name": name, "type": rtype, "ttl": 300, "data": data })
                            })
                            .collect();
                        json!({ "Response": {
                            "records": records,
                            "response_time": time,
                            "valid_until": "2099-01-01T00:00:00Z"
                        }})
                    }
                    Some(Answer::ServFail) => json!({ "NxDomain": {
                        "response_time": time, "response_code": "SERVFAIL"
                    }}),
                    None => json!({ "NxDomain": {
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
            serde_json::from_value(json!({ "lookups": lookups })).expect("zone lookups decode");
        Box::pin(async move { Ok(decoded) })
    }
}

// ---------------------------------------------------------------------------
// Record data
// ---------------------------------------------------------------------------

fn soa() -> Json {
    json!({ "SOA": {
        "mname": "ns1.example.net.", "rname": "hostmaster.example.net.",
        "serial": 1, "refresh": 7200, "retry": 3600, "expire": 1_209_600, "minimum": 300
    }})
}

fn ns(target: &str) -> Json {
    json!({ "NS": target })
}

fn caa(value: &str) -> Json {
    json!({ "CAA": { "issuer_critical": false, "tag": "issue", "value": value } })
}

fn dnskey_ksk() -> Json {
    json!({ "DNSKEY": {
        "flags": 257, "protocol": 3, "algorithm": "EcdsaP256Sha256",
        "public_key": "mdsswUyr3DPW132mOi8V9xESWE8jTo0dxCjjnopKl+GqJxpVXckHAeF+KkxLbxILfDLUT0rAK9iUzy1L53eKGQ==",
        "key_tag": 2371, "is_zone_key": true, "is_secure_entry_point": true, "is_revoked": false
    }})
}

fn ds() -> Json {
    json!({ "DS": {
        "key_tag": 2371, "algorithm": "EcdsaP256Sha256", "digest_type": "Sha256",
        "digest": "AABBCCDD"
    }})
}

fn rrsig() -> Json {
    json!({ "RRSIG": {
        "type_covered": "A", "algorithm": "EcdsaP256Sha256", "labels": 3,
        "original_ttl": 300, "expiration": 4_102_444_800u64, "inception": 1_700_000_000u64,
        "key_tag": 45104, "signer_name": "example.com.", "signature": "c2ln"
    }})
}

fn nsec() -> Json {
    json!({ "NSEC": { "next_domain_name": "zzz.example.com.", "types": ["A", "RRSIG", "NSEC"] } })
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

struct Ran {
    events: Vec<CheckEvent>,
    outcome: SectionOutcome,
    zones: Arc<Zones>,
    sender: Arc<RecordingSender>,
}

/// Raw nameserver queries: every address answers authoritatively; SOA with an SOA and NS with
/// ns1 and ns2 of example.net, both owned by the queried name.
fn authoritative(_: SocketAddr, name: &Name, rtype: RecordType) -> Option<Message> {
    let owner = name.to_ascii();
    let mut m = reply(true);
    match rtype {
        RecordType::SOA => {
            m.add_answer(soa_record(&owner));
        }
        RecordType::NS => {
            m.add_answer(ns_record(&owner, "ns1.example.net."));
            m.add_answer(ns_record(&owner, "ns2.example.net."));
        }
        _ => return None,
    }
    Some(m)
}

async fn run(domain: &str, zones: Zones) -> Ran {
    let zones = Arc::new(zones);
    let group = ResolverGroupBuilder::new()
        .nameserver(NameServerConfig::udp(
            "192.0.2.53:53".parse::<SocketAddr>().unwrap(),
        ))
        .nameserver(NameServerConfig::udp(
            "192.0.2.54:53".parse::<SocketAddr>().unwrap(),
        ))
        .build()
        .await
        .expect("resolver group over documentation addresses");
    let config = crate::config::Config::load(None).expect("default config");
    let sender = RecordingSender::new(authoritative);
    let glue = StubResolver::new(&[
        ("ns1.example.net", &[v4(192, 0, 2, 53)]),
        ("ns2.example.net", &[v4(192, 0, 2, 54)]),
    ]);
    let backend = DnsBackend {
        lookup: Arc::new(ZonesLookup(Arc::clone(&zones))),
        raw: test_outbound(sender.clone(), glue),
    };
    let outbound = Outbound {
        settings: ClientSettings::default(),
        resolver: StubResolver::new(&[]),
        allow: |_| false,
    };
    let run = CheckRun {
        request_id: "zone-test".to_owned(),
        domain: domain.to_owned(),
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
    let outcome = translate(&events);
    Ran {
        events,
        outcome,
        zones,
        sender,
    }
}

impl Ran {
    fn parts(&self) -> (&[netray_model::CheckResult], &[Block]) {
        match &self.outcome {
            SectionOutcome::Measured { checks, evidence } => (checks, evidence),
            other => panic!("section not measured: {other:?}"),
        }
    }

    /// Status and findings of `dns.<name>`.
    fn check(&self, name: &str) -> (Status, Vec<String>) {
        let id = CheckId::parse(&format!("dns.{name}")).unwrap();
        let (checks, _) = self.parts();
        let found = checks
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no check dns.{name}"));
        (found.status.clone(), found.findings.clone())
    }

    /// The (owner, data) rows of block `dns.<rtype>`, `None` when the section has none.
    fn block(&self, rtype: &str) -> Option<Vec<(String, String)>> {
        let id = block_id(rtype);
        let (_, evidence) = self.parts();
        let block = evidence.iter().find(|b| b.id == id)?;
        let Shape::Records(records) = &block.shape else {
            panic!("block dns.{rtype} is not records");
        };
        let text = |v: &Value| match v {
            Value::Text(t) => t.clone(),
            other => panic!("not text: {other:?}"),
        };
        Some(
            records
                .iter()
                .map(|r| (key(&text(&r.owner)), text(&r.data)))
                .collect(),
        )
    }

    /// Owners of the records of every batch of `record_type`.
    fn batch_owners(&self, record_type: &str) -> Vec<String> {
        let mut owners = BTreeSet::new();
        for event in &self.events {
            if let CheckEvent::Batch(batch) = event
                && batch.record_type == record_type
            {
                for lookup in batch.lookups.iter() {
                    for record in lookup.records() {
                        owners.insert(key(&record.name().to_string()));
                    }
                }
            }
        }
        owners.into_iter().collect()
    }
}

fn assert_unmeasured_naming(ran: &Ran, checks: &[&str], name: &str, rtype: &str) {
    for check in checks {
        let (status, findings) = ran.check(check);
        assert_eq!(status, Status::Unmeasured, "dns.{check}: {findings:?}");
        assert!(
            findings
                .iter()
                .any(|f| f.contains(name) && f.contains(rtype)),
            "dns.{check} must name the {name} {rtype} lookup: {findings:?}"
        );
    }
}

const NS_CHECKS: [&str; 3] = ["ns", "ns_lame", "ns_delegation"];
const DNSSEC_CHECKS: [&str; 3] = ["dnssec", "dnskey_algorithm", "dnssec_rollover"];

// ---------------------------------------------------------------------------
// C1, C5, C6, C7: which name is the zone
// ---------------------------------------------------------------------------

/// C5 (and C1): the zone of www.example.com is example.com; its SOA reaches no SOA batch.
#[tokio::test]
async fn c5_zone_is_the_ancestor_and_its_soa_sends_no_batch() {
    let zones = Zones::default()
        .with("example.com", "SOA", vec![soa()])
        .with(
            "example.com",
            "NS",
            vec![ns("ns1.example.net."), ns("ns2.example.net.")],
        );
    let ran = run("www.example.com", zones).await;

    assert!(ran.zones.was_asked("example.com", "SOA"));
    assert_eq!(ran.batch_owners("SOA"), Vec::<String>::new());
    // The zone was found: the NS checks judge example.com, so they are not "Not found".
    assert_eq!(ran.check("ns").0, Status::Pass);
}

/// C6 (and C1): an apex owning its SOA is the zone, and no ancestor is queried.
#[tokio::test]
async fn c6_apex_is_its_own_zone_and_queries_no_ancestor() {
    let zones = Zones::default()
        .with("example.com", "SOA", vec![soa()])
        .with(
            "example.com",
            "NS",
            vec![ns("ns1.example.net."), ns("ns2.example.net.")],
        );
    let ran = run("example.com", zones).await;

    assert!(!ran.zones.asked_name("com"));
    assert_eq!(ran.batch_owners("SOA"), vec!["example.com".to_owned()]);
    assert_eq!(ran.check("ns").0, Status::Pass);
}

/// C7: two ancestor lookups find example.com; the TLD is never queried.
#[tokio::test]
async fn c7_zone_found_after_two_ancestor_lookups_never_the_tld() {
    let zones = Zones::default()
        .with("example.com", "SOA", vec![soa()])
        .with(
            "example.com",
            "NS",
            vec![ns("ns1.example.net."), ns("ns2.example.net.")],
        );
    let ran = run("a.b.example.com", zones).await;

    assert!(ran.zones.was_asked("b.example.com", "SOA"));
    assert!(ran.zones.was_asked("example.com", "SOA"));
    assert!(!ran.zones.asked_name("com"));
    assert_eq!(ran.batch_owners("SOA"), Vec::<String>::new());
    assert_eq!(ran.check("ns").0, Status::Pass);
}

/// C1: when every ancestor answers a definite negative, the checks judge the name itself.
#[tokio::test]
async fn c1_all_negative_ancestors_keep_judging_the_name() {
    let zones = Zones::default().with(
        "www.example.com",
        "NS",
        vec![ns("ns1.example.net."), ns("ns2.example.net.")],
    );
    let ran = run("www.example.com", zones).await;

    assert!(ran.zones.was_asked("example.com", "SOA"));
    assert!(!ran.zones.asked_name("com"));
    for check in NS_CHECKS {
        assert_eq!(ran.check(check).0, Status::Pass, "dns.{check}");
    }
    assert!(
        ran.sender
            .sent()
            .iter()
            .all(|s| key(&s.name.to_ascii()) == "www.example.com")
    );
}

// ---------------------------------------------------------------------------
// C2, C8, C9: NS
// ---------------------------------------------------------------------------

/// C8: an ancestor SOA that SERVFAILs on every resolver leaves the zone unknown.
#[tokio::test]
async fn c8_servfailed_ancestor_soa_makes_zone_checks_unmeasured() {
    let zones = Zones::default().servfail("example.com", "SOA");
    let ran = run("www.example.com", zones).await;

    let checks: Vec<&str> = NS_CHECKS
        .iter()
        .chain(DNSSEC_CHECKS.iter())
        .copied()
        .collect();
    assert_unmeasured_naming(&ran, &checks, "example.com", "SOA");
}

/// C9: the NS checks of www.example.com judge example.com's nameservers and block.
#[tokio::test]
async fn c9_ns_checks_judge_the_zone() {
    let zones = Zones::default()
        .with("example.com", "SOA", vec![soa()])
        .with(
            "example.com",
            "NS",
            vec![ns("ns1.example.net."), ns("ns2.example.net.")],
        );
    let ran = run("www.example.com", zones).await;

    for check in NS_CHECKS {
        assert_eq!(ran.check(check).0, Status::Pass, "dns.{check}");
    }
    let sent = ran.sender.sent();
    assert!(
        !sent.is_empty(),
        "the lame and consistency queries were sent"
    );
    assert!(
        sent.iter()
            .all(|s| key(&s.name.to_ascii()) == "example.com"),
        "raw queries ask for the zone: {sent:?}"
    );
    assert_eq!(
        ran.block("NS").expect("dns.ns block"),
        vec![
            ("example.com".to_owned(), "ns1.example.net.".to_owned()),
            ("example.com".to_owned(), "ns2.example.net.".to_owned()),
        ]
    );
}

// ---------------------------------------------------------------------------
// C3, C10, C11: DNSSEC
// ---------------------------------------------------------------------------

/// C10: a name in a signed zone is judged by the zone's DNSKEY and DS.
#[tokio::test]
async fn c10_dnssec_checks_judge_the_zone_keys() {
    let zones = Zones::default()
        .with("example.com", "SOA", vec![soa()])
        .with("example.com", "DNSKEY", vec![dnskey_ksk()])
        .with("example.com", "DS", vec![ds()])
        .with("www.example.com", "RRSIG", vec![rrsig()])
        .with("www.example.com", "NSEC", vec![nsec()]);
    let ran = run("www.example.com", zones).await;

    for check in DNSSEC_CHECKS {
        let (status, findings) = ran.check(check);
        assert_eq!(status, Status::Pass, "dns.{check}: {findings:?}");
    }
    let dnskey = ran.block("DNSKEY").expect("dns.dnskey block");
    assert!(!dnskey.is_empty() && dnskey.iter().all(|(owner, _)| owner == "example.com"));
    let ds = ran.block("DS").expect("dns.ds block");
    assert!(!ds.is_empty() && ds.iter().all(|(owner, _)| owner == "example.com"));
}

/// C11: an unsigned subdomain of an unsigned zone reads as it does for an apex.
#[tokio::test]
async fn c11_unsigned_zone_reads_not_deployed() {
    let zones = Zones::default().with("example.com", "SOA", vec![soa()]);
    let ran = run("www.example.com", zones).await;

    let (status, findings) = ran.check("dnssec");
    assert_eq!(status, Status::Warn);
    assert!(
        findings
            .iter()
            .any(|f| f.contains("No DNSSEC records found")),
        "{findings:?}"
    );
    for check in ["dnskey_algorithm", "dnssec_rollover"] {
        let (status, findings) = ran.check(check);
        assert_eq!(status, Status::NotApplicable, "dns.{check}");
        assert_eq!(findings, vec!["DNSSEC not deployed".to_owned()]);
    }
}

// ---------------------------------------------------------------------------
// C4, C12, C13, C14, C15: CAA
// ---------------------------------------------------------------------------

/// C12: the CAA of the zone judges the subdomain; the block holds it under the ancestor.
#[tokio::test]
async fn c12_caa_climbs_to_the_zone_and_the_block_holds_it() {
    let zones = Zones::default()
        .with("example.com", "SOA", vec![soa()])
        .with("example.com", "CAA", vec![caa("ca.example.net")]);
    let ran = run("www.example.com", zones).await;

    let (status, findings) = ran.check("caa");
    assert_eq!(status, Status::Pass, "{findings:?}");
    assert!(!findings.iter().any(|f| f.contains("No CAA records found")));
    let rows = ran.block("CAA").expect("dns.caa block");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "example.com");
    assert!(rows[0].1.contains("ca.example.net"), "{rows:?}");
}

/// C13: the climb asks every ancestor below the TLD, in turn, and never the TLD.
#[tokio::test]
async fn c13_caa_climb_asks_each_ancestor_never_the_tld() {
    let zones = Zones::default()
        .with("example.com", "SOA", vec![soa()])
        .with("example.com", "CAA", vec![caa("ca.example.net")]);
    let ran = run("a.b.example.com", zones).await;

    assert!(ran.zones.was_asked("a.b.example.com", "CAA"));
    assert!(ran.zones.was_asked("b.example.com", "CAA"));
    assert!(ran.zones.was_asked("example.com", "CAA"));
    assert!(!ran.zones.asked_name("com"));
}

/// C14: no CAA at the name nor any ancestor keeps the warning.
#[tokio::test]
async fn c14_no_caa_anywhere_keeps_the_warning() {
    let zones = Zones::default().with("example.com", "SOA", vec![soa()]);
    let ran = run("www.example.com", zones).await;

    let (status, findings) = ran.check("caa");
    assert_eq!(status, Status::Warn);
    assert!(
        findings.iter().any(|f| f.contains("No CAA records found")),
        "{findings:?}"
    );
}

/// C15: an ancestor CAA lookup with no definite answer makes dns.caa unmeasured.
#[tokio::test]
async fn c15_failing_ancestor_caa_lookup_is_unmeasured() {
    let zones = Zones::default()
        .with("example.com", "SOA", vec![soa()])
        .servfail("example.com", "CAA");
    let ran = run("www.example.com", zones).await;

    assert_unmeasured_naming(&ran, &["caa"], "example.com", "CAA");
}

// ---------------------------------------------------------------------------
// C16: the apex is unchanged
// ---------------------------------------------------------------------------

/// C16: for a zone apex the lookups made are today's 15 types and 4 subdomain TXT, nothing else.
#[tokio::test]
async fn c16_apex_makes_exactly_todays_lookups() {
    let zones = Zones::default()
        .with("example.com", "SOA", vec![soa()])
        .with(
            "example.com",
            "NS",
            vec![ns("ns1.example.net."), ns("ns2.example.net.")],
        );
    let ran = run("example.com", zones).await;

    let mut expected: BTreeSet<(String, String)> = [
        "A", "AAAA", "CAA", "CNAME", "DNSKEY", "DS", "HTTPS", "MX", "NS", "NSEC", "NSEC3", "RRSIG",
        "SOA", "SVCB", "TXT",
    ]
    .iter()
    .map(|t| ("example.com".to_owned(), (*t).to_owned()))
    .collect();
    for sub in [
        "_dmarc.example.com",
        "default._bimi.example.com",
        "_mta-sts.example.com",
        "_smtp._tls.example.com",
    ] {
        expected.insert((sub.to_owned(), "TXT".to_owned()));
    }
    assert_eq!(ran.zones.asked(), expected);
    assert_eq!(ran.check("ns").0, Status::Pass);
    assert_eq!(ran.check("caa").0, Status::Warn);
}
