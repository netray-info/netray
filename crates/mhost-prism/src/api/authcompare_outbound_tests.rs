//! The authoritative comparison through the outbound context (requirement 5 of
//! `specs/features/raw-query-policy/spec.md`): nameserver addresses the outbound policy
//! refuses are not queried and are reported once; allowed addresses are queried and labelled
//! as before. Nothing here touches the network.

use std::net::SocketAddr;
use std::time::Duration;

use hickory_proto::rr::RecordType;

use super::{query_auth_servers, resolve_auth_servers};
use crate::dns_raw::NOT_PUBLIC;
use crate::dns_raw::outbound_tests::{
    RecordingSender, StubResolver, name, reply, test_outbound, v4,
};

const TIMEOUT: Duration = Duration::from_millis(50);

fn ns(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

fn count_not_public(warnings: &[String]) -> usize {
    warnings.iter().filter(|w| w.as_str() == NOT_PUBLIC).count()
}

#[test]
fn not_public_wording_is_fixed() {
    assert_eq!(NOT_PUBLIC, "nameserver address not public, not queried");
}

#[tokio::test]
async fn refused_glue_is_dropped_warned_once_and_not_queried() {
    let sender = RecordingSender::new(|_, _, _| Some(reply(true)));
    let resolver = StubResolver::new(&[("ns1.example.com", &[v4(172, 16, 0, 5)])]);
    let raw = test_outbound(sender.clone(), resolver);

    let auth = resolve_auth_servers(&raw, &ns(&["ns1.example.com."])).await;

    assert!(
        auth.servers.iter().all(|s| s.ip() != v4(172, 16, 0, 5)),
        "refused address must not be listed: {:?}",
        auth.servers
    );
    assert!(auth.labels.iter().all(|l| !l.contains("172.16.0.5")));
    assert_eq!(auth.warnings.len(), 1);
    assert_eq!(count_not_public(&auth.warnings), 1);

    query_auth_servers(
        &raw,
        &auth.servers,
        &name("example.com."),
        RecordType::A,
        TIMEOUT,
    )
    .await;
    assert!(
        !sender.recorded_ips().contains(&v4(172, 16, 0, 5)),
        "refused address received a query: {:?}",
        sender.recorded_ips()
    );
}

#[tokio::test]
async fn several_refused_names_warn_exactly_once() {
    let sender = RecordingSender::new(|_, _, _| Some(reply(true)));
    let resolver = StubResolver::new(&[
        ("ns1.example.com", &[v4(172, 16, 0, 5)]),
        ("ns2.example.com", &[v4(10, 0, 0, 1)]),
    ]);
    let raw = test_outbound(sender.clone(), resolver);

    let auth = resolve_auth_servers(&raw, &ns(&["ns1.example.com.", "ns2.example.com."])).await;

    assert!(
        auth.servers.is_empty(),
        "no server may remain: {:?}",
        auth.servers
    );
    assert_eq!(count_not_public(&auth.warnings), 1);
    assert_eq!(auth.warnings.len(), 1);

    query_auth_servers(
        &raw,
        &auth.servers,
        &name("example.com."),
        RecordType::A,
        TIMEOUT,
    )
    .await;
    assert!(sender.recorded_ips().is_empty());
}

#[tokio::test]
async fn allowed_glue_is_queried_labelled_and_returned() {
    let sender = RecordingSender::new(|_, _, _| Some(reply(true)));
    let resolver = StubResolver::new(&[("ns1.example.com", &[v4(192, 0, 2, 53)])]);
    let raw = test_outbound(sender.clone(), resolver);

    let auth = resolve_auth_servers(&raw, &ns(&["ns1.example.com."])).await;

    let expected: SocketAddr = "192.0.2.53:53".parse().unwrap();
    assert_eq!(auth.servers, vec![expected]);
    assert_eq!(
        auth.labels,
        vec!["ns1.example.com. (192.0.2.53)".to_owned()]
    );
    assert!(
        auth.warnings.is_empty(),
        "unexpected warnings: {:?}",
        auth.warnings
    );

    let results = query_auth_servers(
        &raw,
        &auth.servers,
        &name("example.com."),
        RecordType::A,
        TIMEOUT,
    )
    .await;

    let sent = sender.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].server, expected);
    assert!(!sent[0].dnssec_ok);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].server, expected);
    assert!(results[0].result.is_ok());
}

#[tokio::test]
async fn mixed_glue_keeps_only_the_allowed_address() {
    let sender = RecordingSender::new(|_, _, _| Some(reply(true)));
    let resolver = StubResolver::new(&[
        ("ns1.example.com", &[v4(172, 16, 0, 5)]),
        ("ns2.example.com", &[v4(192, 0, 2, 53)]),
    ]);
    let raw = test_outbound(sender.clone(), resolver);

    let auth = resolve_auth_servers(&raw, &ns(&["ns1.example.com.", "ns2.example.com."])).await;

    let allowed: SocketAddr = "192.0.2.53:53".parse().unwrap();
    assert_eq!(auth.servers, vec![allowed]);
    assert_eq!(count_not_public(&auth.warnings), 1);
    assert_eq!(auth.warnings.len(), 1);

    query_auth_servers(
        &raw,
        &auth.servers,
        &name("example.com."),
        RecordType::A,
        TIMEOUT,
    )
    .await;
    assert_eq!(sender.recorded_ips(), vec![v4(192, 0, 2, 53)]);
}
