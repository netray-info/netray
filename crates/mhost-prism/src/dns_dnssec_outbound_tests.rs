//! The DNSSEC chain walk under the outbound policy (requirement 4 of
//! `specs/features/raw-query-policy/spec.md`). Every query, root servers included, goes
//! through a recording sender; nothing touches the network.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use super::{ChainLevel, walk_chain_with};
use crate::dns_raw::NOT_PUBLIC;
use crate::dns_raw::outbound_tests::{
    RecordingSender, StubResolver, is_root, name, referral, reply, test_outbound, v4, v6,
};
use hickory_proto::rr::RecordType;

/// Walks `example.com.`. Root servers answer DNSKEY/DS with an empty reply and the NS query
/// for `com.` with a referral naming `ns` and carrying `glue`. Other servers answer DNSKEY
/// with an empty authoritative reply and have no delegation for `example.com.`.
async fn walk(
    ns: &'static [&'static str],
    glue: Vec<(&'static str, IpAddr)>,
    resolver_entries: &[(&str, &[IpAddr])],
) -> (Vec<ChainLevel>, Arc<RecordingSender>, Arc<StubResolver>) {
    let sender = RecordingSender::new(move |server, qname, rtype| {
        if is_root(server.ip()) {
            if rtype == RecordType::NS && *qname == name("com.") {
                Some(referral("com.", ns, &glue))
            } else {
                Some(reply(true))
            }
        } else {
            Some(reply(true))
        }
    });
    let resolver = StubResolver::new(resolver_entries);
    let raw = test_outbound(sender.clone(), resolver.clone());
    let levels = walk_chain_with(&raw, name("example.com."), 10, Duration::from_secs(1)).await;
    (levels, sender, resolver)
}

fn has_not_public(level: &ChainLevel) -> bool {
    level
        .findings
        .iter()
        .any(|f| f.severity == "warning" && f.message == NOT_PUBLIC)
}

#[tokio::test]
async fn chain_walk_refuses_not_public_glue_and_stops_with_a_warning() {
    let (levels, sender, _) = walk(
        &["a.gtld.example.com."],
        vec![
            ("a.gtld.example.com.", v4(10, 0, 0, 1)),
            ("a.gtld.example.com.", v6("2001:db8::1")),
        ],
        &[],
    )
    .await;

    let ips = sender.recorded_ips();
    assert!(!ips.contains(&v4(10, 0, 0, 1)), "queried: {ips:?}");
    assert!(!ips.contains(&v6("2001:db8::1")), "queried: {ips:?}");
    assert!(
        ips.iter().any(|ip| is_root(*ip)),
        "root servers go through the sender"
    );

    let last = levels.last().expect("levels");
    assert_eq!(last.zone, "com.");
    assert_eq!(last.servers_queried, 0);
    assert!(has_not_public(last), "findings: {:?}", last.findings);
    assert_eq!(levels.iter().filter(|l| l.zone == "com.").count(), 1);
    assert!(
        levels.iter().all(|l| l.zone != "example.com."),
        "walk stopped"
    );
}

#[tokio::test]
async fn chain_walk_does_not_resolve_a_name_that_has_refused_glue() {
    let (levels, sender, resolver) = walk(
        &["a.gtld.example.com."],
        vec![("a.gtld.example.com.", v4(10, 0, 0, 1))],
        &[("a.gtld.example.com", &[v4(192, 0, 2, 53)])],
    )
    .await;

    let ips = sender.recorded_ips();
    assert!(!ips.contains(&v4(10, 0, 0, 1)), "queried: {ips:?}");
    assert!(!ips.contains(&v4(192, 0, 2, 53)), "queried: {ips:?}");
    assert!(
        resolver.asked().iter().all(|h| h != "a.gtld.example.com"),
        "asked: {:?}",
        resolver.asked()
    );
    let com = levels
        .iter()
        .find(|l| l.zone == "com.")
        .expect("com. level");
    assert!(has_not_public(com), "findings: {:?}", com.findings);
}

#[tokio::test]
async fn chain_walk_queries_only_the_public_glue_and_drops_refused_silently() {
    let (levels, sender, _) = walk(
        &["ns1.example.com.", "ns2.example.com."],
        vec![
            ("ns1.example.com.", v4(10, 0, 0, 1)),
            ("ns2.example.com.", v4(192, 0, 2, 53)),
        ],
        &[],
    )
    .await;

    let below_root: Vec<IpAddr> = sender
        .recorded_ips()
        .into_iter()
        .filter(|ip| !is_root(*ip))
        .collect();
    assert!(!below_root.is_empty());
    assert!(
        below_root.iter().all(|ip| *ip == v4(192, 0, 2, 53)),
        "queried: {below_root:?}"
    );
    assert!(
        levels.iter().all(|l| !has_not_public(l)),
        "levels: {levels:?}"
    );
}

#[tokio::test]
async fn chain_walk_queries_public_glue_without_a_warning() {
    let (levels, sender, _) = walk(
        &["ns1.example.com."],
        vec![("ns1.example.com.", v4(192, 0, 2, 53))],
        &[],
    )
    .await;

    assert!(sender.recorded_ips().contains(&v4(192, 0, 2, 53)));
    let com = levels
        .iter()
        .find(|l| l.zone == "com.")
        .expect("com. level");
    assert_eq!(com.servers_queried, 1);
    assert!(
        levels.iter().all(|l| !has_not_public(l)),
        "levels: {levels:?}"
    );
}
