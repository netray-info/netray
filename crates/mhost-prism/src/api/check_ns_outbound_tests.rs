//! The NS checks send their raw queries through the outbound context (requirement 3 of
//! `specs/features/raw-query-policy/spec.md`): an address the outbound policy refuses is not
//! queried and is reported once per check as a warning; the other addresses are judged as before.
//!
//! Nothing here touches the network: every query goes through the recording sender, and the
//! test `allow` admits only 192.0.2.x as stand-ins for public addresses.

use std::net::IpAddr;
use std::time::Duration;

use hickory_proto::op::Message;
use hickory_proto::rr::{Name, RecordType};
use mhost::lints::CheckResult;
use mhost::resolver::Lookups;

use super::{check_ns_delegation_consistency, check_ns_lame_delegation};
use crate::dns_raw::NOT_PUBLIC;
use crate::dns_raw::outbound_tests::{
    RecordingSender, StubResolver, name, ns_record, reply, soa_record, test_outbound, v4,
};

const DOMAIN: &str = "example.com";
const TIMEOUT: Duration = Duration::from_secs(1);

/// `example.com` with one NS record per name, built through serde like `tests::make_txt_lookups`.
fn ns_lookups(ns_names: &[&str]) -> Lookups {
    let records: Vec<serde_json::Value> = ns_names
        .iter()
        .map(|ns| {
            serde_json::json!({
                "name": DOMAIN,
                "type": "NS",
                "ttl": 300,
                "data": { "NS": ns }
            })
        })
        .collect();
    serde_json::from_value(serde_json::json!({
        "lookups": [{
            "query": { "name": DOMAIN, "record_type": "NS" },
            "name_server": "udp:127.0.0.1:53",
            "result": {
                "Response": {
                    "records": records,
                    "response_time": { "secs": 0, "nanos": 10000000 },
                    "valid_until": "2099-01-01T00:00:00Z"
                }
            }
        }]
    }))
    .expect("test Lookups")
}

/// SOA queries: 192.0.2.53 answers AA=1, every other server AA=0. NS queries: AA=1 with both names.
fn answer(server: std::net::SocketAddr, _: &Name, rtype: RecordType) -> Option<Message> {
    match rtype {
        RecordType::SOA => {
            if server.ip() == v4(192, 0, 2, 53) {
                let mut m = reply(true);
                m.add_answer(soa_record("example.com."));
                Some(m)
            } else {
                Some(reply(false))
            }
        }
        RecordType::NS => {
            let mut m = reply(true);
            m.add_answer(ns_record("example.com.", "ns1.example.com."));
            m.add_answer(ns_record("example.com.", "ns2.example.com."));
            Some(m)
        }
        _ => None,
    }
}

fn warning(msg: &str) -> CheckResult {
    CheckResult::Warning(msg.to_owned())
}

fn sorted(mut results: Vec<CheckResult>) -> Vec<CheckResult> {
    results.sort_by_key(|r| format!("{r:?}"));
    results
}

fn sorted_ips(mut ips: Vec<IpAddr>) -> Vec<IpAddr> {
    ips.sort();
    ips
}

fn count_not_public(results: &[CheckResult]) -> usize {
    results
        .iter()
        .filter(|r| **r == warning(NOT_PUBLIC))
        .count()
}

#[tokio::test]
async fn lame_check_with_only_a_refused_address_sends_nothing_and_warns() {
    let sender = RecordingSender::new(answer);
    let resolver = StubResolver::new(&[("ns1.example.com", &[v4(10, 0, 0, 1)])]);
    let raw = test_outbound(sender.clone(), resolver);
    let lookups = ns_lookups(&["ns1.example.com."]);

    let results = check_ns_lame_delegation(&lookups, DOMAIN, TIMEOUT, &raw).await;

    assert!(sender.recorded_ips().is_empty());
    assert_eq!(results, vec![warning(NOT_PUBLIC)]);
}

#[tokio::test]
async fn lame_check_queries_the_allowed_address_and_warns_about_the_refused_one() {
    let sender = RecordingSender::new(answer);
    let resolver = StubResolver::new(&[
        ("ns1.example.com", &[v4(10, 0, 0, 1)]),
        ("ns2.example.com", &[v4(192, 0, 2, 53)]),
    ]);
    let raw = test_outbound(sender.clone(), resolver);
    let lookups = ns_lookups(&["ns1.example.com.", "ns2.example.com."]);

    let results = check_ns_lame_delegation(&lookups, DOMAIN, TIMEOUT, &raw).await;

    assert_eq!(sender.recorded_ips(), vec![v4(192, 0, 2, 53)]);
    assert_eq!(
        sorted(results),
        sorted(vec![
            warning(NOT_PUBLIC),
            CheckResult::Ok("All 1 NS server(s) answered authoritatively (AA=1)".to_owned()),
        ])
    );
}

#[tokio::test]
async fn lame_check_warns_once_when_every_address_is_refused() {
    let sender = RecordingSender::new(answer);
    let resolver = StubResolver::new(&[
        ("ns1.example.com", &[v4(10, 0, 0, 1)]),
        ("ns2.example.com", &[v4(10, 0, 0, 2)]),
    ]);
    let raw = test_outbound(sender.clone(), resolver);
    let lookups = ns_lookups(&["ns1.example.com.", "ns2.example.com."]);

    let results = check_ns_lame_delegation(&lookups, DOMAIN, TIMEOUT, &raw).await;

    assert!(sender.recorded_ips().is_empty());
    assert_eq!(count_not_public(&results), 1);
    assert_eq!(results, vec![warning(NOT_PUBLIC)]);
}

#[tokio::test]
async fn delegation_check_queries_the_allowed_address_and_warns_once_about_the_refused_one() {
    let sender = RecordingSender::new(answer);
    let resolver = StubResolver::new(&[
        ("ns1.example.com", &[v4(10, 0, 0, 1)]),
        ("ns2.example.com", &[v4(192, 0, 2, 53)]),
    ]);
    let raw = test_outbound(sender.clone(), resolver);
    let lookups = ns_lookups(&["ns1.example.com.", "ns2.example.com."]);

    let results = check_ns_delegation_consistency(&lookups, DOMAIN, TIMEOUT, &raw).await;

    assert_eq!(sender.recorded_ips(), vec![v4(192, 0, 2, 53)]);
    assert_eq!(count_not_public(&results), 1);
    assert_eq!(
        sorted(results),
        sorted(vec![
            warning(NOT_PUBLIC),
            CheckResult::Ok(
                "NS delegation is consistent: 2 name server(s) match between parent and child"
                    .to_owned()
            ),
        ])
    );
}

#[tokio::test]
async fn delegation_check_with_only_a_refused_address_yields_just_the_warning() {
    let sender = RecordingSender::new(answer);
    let resolver = StubResolver::new(&[("ns1.example.com", &[v4(10, 0, 0, 1)])]);
    let raw = test_outbound(sender.clone(), resolver);
    let lookups = ns_lookups(&["ns1.example.com."]);

    let results = check_ns_delegation_consistency(&lookups, DOMAIN, TIMEOUT, &raw).await;

    assert!(sender.recorded_ips().is_empty());
    assert_eq!(results, vec![warning(NOT_PUBLIC)]);
}

#[tokio::test]
async fn lame_check_with_only_allowed_addresses_judges_as_before_without_a_warning() {
    let sender = RecordingSender::new(answer);
    let resolver = StubResolver::new(&[
        ("ns1.example.com", &[v4(192, 0, 2, 53)]),
        ("ns2.example.com", &[v4(192, 0, 2, 54)]),
    ]);
    let raw = test_outbound(sender.clone(), resolver);
    let lookups = ns_lookups(&["ns1.example.com.", "ns2.example.com."]);

    let results = check_ns_lame_delegation(&lookups, DOMAIN, TIMEOUT, &raw).await;

    assert_eq!(
        sorted_ips(sender.recorded_ips()),
        vec![v4(192, 0, 2, 53), v4(192, 0, 2, 54)]
    );
    assert_eq!(count_not_public(&results), 0);
    assert_eq!(
        sorted(results),
        sorted(vec![
            CheckResult::Failed(
                "NS server 192.0.2.54 is lame: answered with AA=0 (not authoritative for example.com)"
                    .to_owned()
            ),
            warning("1 NS server(s) answered correctly; 1 server(s) lame"),
        ])
    );
}

#[tokio::test]
async fn delegation_check_with_only_allowed_addresses_judges_as_before_without_a_warning() {
    let sender = RecordingSender::new(answer);
    let resolver = StubResolver::new(&[
        ("ns1.example.com", &[v4(192, 0, 2, 53)]),
        ("ns2.example.com", &[v4(192, 0, 2, 54)]),
    ]);
    let raw = test_outbound(sender.clone(), resolver);
    let lookups = ns_lookups(&["ns1.example.com.", "ns2.example.com."]);

    let results = check_ns_delegation_consistency(&lookups, DOMAIN, TIMEOUT, &raw).await;

    assert_eq!(
        sorted_ips(sender.recorded_ips()),
        vec![v4(192, 0, 2, 53), v4(192, 0, 2, 54)]
    );
    assert_eq!(
        results,
        vec![CheckResult::Ok(
            "NS delegation is consistent: 2 name server(s) match between parent and child"
                .to_owned()
        )]
    );
}
