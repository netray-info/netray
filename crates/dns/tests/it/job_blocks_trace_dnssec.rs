//! V2 spec v2-dns-ui, requirements 14, 15 (data half): `trace_blocks` and `dnssec_blocks` carry
//! the answer records, error messages, server names, referral names and DNSSEC record rdata.

use std::net::IpAddr;

use netray_dns::blocks::{dnssec_blocks, trace_blocks};
use netray_dns::config::Config;
use netray_dns::dns_dnssec::{ChainFinding, ChainLevel};
use netray_dns::dns_raw::DnsRecord;
use netray_dns::dns_trace::{ReferralGroup, ServerOutcome, ServerResult, TraceHop};
use netray_dns::security::RateLimitState;
use netray_dns::{
    DnsChainEvent, DnsDnssecResult, DnsHopEvent, DnsTraceResult, DnssecRequest, TraceRequest,
    testing,
};
use netray_model::evidence::{Node, Shape, Value};
use serde_json::json;

fn client() -> IpAddr {
    "203.0.113.7".parse().unwrap()
}

fn limiter() -> RateLimitState {
    RateLimitState::new(&Config::load(None).expect("default config").limits)
}

fn tree(shape: &Shape) -> &Node {
    let Shape::Tree(root) = shape else {
        panic!("not a tree: {shape:?}")
    };
    root
}

fn texts<'a>(node: &'a Node, key: &str) -> Vec<&'a str> {
    node.fields
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| match v {
            Value::Text(t) => t.as_str(),
            other => panic!("{key} is not text: {other:?}"),
        })
        .collect()
}

fn walk<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(node);
    for child in &node.children {
        walk(child, out);
    }
}

fn record(record_type: &str, rdata: &str) -> DnsRecord {
    DnsRecord {
        name: "example.com.".into(),
        ttl: 300,
        record_type: record_type.into(),
        rdata: rdata.into(),
    }
}

#[tokio::test]
async fn stub_trace_of_mx_carries_the_answer_and_no_empty_text() {
    let req: TraceRequest =
        serde_json::from_value(json!({"domain": "example.com", "record_type": "MX"}))
            .expect("trace request parses");
    let result = testing::stub_jobs()
        .trace(&req, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;

    let blocks = trace_blocks(&result);

    let mut nodes = Vec::new();
    walk(tree(&blocks[0].shape), &mut nodes);
    let answers: Vec<&str> = nodes.iter().flat_map(|n| texts(n, "answer")).collect();
    assert!(
        answers
            .iter()
            .any(|a| a.starts_with("MX ") && a.contains("mx1.example.com.")),
        "answers: {answers:?}"
    );
    for n in &nodes {
        if texts(n, "outcome") == ["error"] {
            assert!(!texts(n, "error").is_empty(), "error server: {n:?}");
        }
        for (k, v) in &n.fields {
            if let Value::Text(t) = v {
                assert!(!t.is_empty(), "empty text under {k}");
            }
        }
    }
}

#[test]
fn hand_built_hop_carries_groups_names_referrals_and_errors() {
    let ok = ServerResult {
        server_ip: "192.0.2.1".parse().unwrap(),
        server_name: Some("a.root-servers.net".into()),
        latency_ms: 12.0,
        outcome: ServerOutcome::Referral,
        answer_records: vec![],
        referral_ns: vec!["ns1.example.com.".into(), "ns2.example.com.".into()],
        authority_zone: None,
    };
    let bad = ServerResult {
        server_ip: "192.0.2.2".parse().unwrap(),
        server_name: None,
        latency_ms: 0.0,
        outcome: ServerOutcome::Error {
            message: "timed out".into(),
        },
        answer_records: vec![],
        referral_ns: vec![],
        authority_zone: None,
    };
    let group = |name: &str, ip: &str| ReferralGroup {
        ns_names: vec![name.into()],
        servers: vec![ip.parse().unwrap()],
        is_majority: false,
    };
    let result = DnsTraceResult {
        hops: vec![DnsHopEvent {
            request_id: "rid".into(),
            hop: TraceHop {
                level: 0,
                zone: ".".into(),
                servers_queried: 2,
                server_results: vec![ok, bad],
                referral_groups: vec![
                    group("ns1.example.com.", "192.0.2.1"),
                    group("ns9.example.com.", "192.0.2.2"),
                ],
                is_final: false,
            },
        }],
        ..Default::default()
    };

    let blocks = trace_blocks(&result);

    let hop = &tree(&blocks[0].shape).children[0];
    assert!(
        hop.fields
            .contains(&("referral_groups".to_string(), Value::Int(2))),
        "hop: {hop:?}"
    );
    let named = &hop.children[0];
    assert_eq!(texts(named, "server_name"), ["a.root-servers.net"]);
    assert_eq!(
        texts(named, "referral"),
        ["ns1.example.com.", "ns2.example.com."]
    );
    assert_eq!(texts(&hop.children[1], "error"), ["timed out"]);
}

#[test]
fn hand_built_level_carries_dnskey_ds_and_rrsig_rdata() {
    let result = DnsDnssecResult {
        levels: vec![DnsChainEvent {
            request_id: "rid".into(),
            level: ChainLevel {
                level: 1,
                zone: "example.com.".into(),
                servers_queried: 1,
                dnskey_records: vec![record("DNSKEY", "257 3 13 AAAA")],
                ds_records: vec![record("DS", "12345 13 2 BBBB")],
                rrsig_records: vec![record("RRSIG", "DNSKEY 13 2 300 CCCC")],
                findings: vec![ChainFinding {
                    severity: "ok".into(),
                    message: "DS matches".into(),
                }],
                latency_ms: 5.0,
                is_final: true,
            },
        }],
        ..Default::default()
    };

    let blocks = dnssec_blocks(&result);

    let level = &tree(&blocks[0].shape).children[0];
    assert_eq!(texts(level, "dnskey"), ["257 3 13 AAAA"]);
    assert_eq!(texts(level, "ds"), ["12345 13 2 BBBB"]);
    assert_eq!(texts(level, "rrsig"), ["DNSKEY 13 2 300 CCCC"]);
}

#[tokio::test]
async fn stub_dnssec_level_nodes_hold_only_contract_keys() {
    let req: DnssecRequest =
        serde_json::from_value(json!({"domain": "example.com"})).expect("dnssec request parses");
    let result = testing::stub_jobs()
        .dnssec(&req, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;

    let blocks = dnssec_blocks(&result);

    for level in &tree(&blocks[0].shape).children {
        for (k, _) in &level.fields {
            assert!(
                ["level", "zone", "latency", "dnskey", "ds", "rrsig"].contains(&k.as_str()),
                "level key {k}"
            );
        }
    }
}
