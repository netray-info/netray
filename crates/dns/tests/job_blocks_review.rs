// V2 Phase 7 review amendments: table key columns are unique and named `lookup`; a node
// carries `latency` only when a latency was measured.

use std::net::IpAddr;

use netray_dns::blocks::{authcompare_blocks, compare_blocks, dnssec_blocks, trace_blocks};
use netray_dns::config::Config;
use netray_dns::dns_dnssec::ChainLevel;
use netray_dns::dns_trace::{ServerOutcome, ServerResult, TraceHop};
use netray_dns::security::RateLimitState;
use netray_dns::{
    DnsChainEvent, DnsDnssecResult, DnsHopEvent, DnsTraceResult, PostQueryRequest, testing,
};
use netray_model::evidence::{Block, Node, Shape};
use serde_json::json;

fn client() -> IpAddr {
    "203.0.113.7".parse().unwrap()
}

fn limiter() -> RateLimitState {
    RateLimitState::new(&Config::default().limits)
}

fn post_many() -> PostQueryRequest {
    serde_json::from_value(json!({"domain": "example.com", "record_types": ["A", "AAAA", "MX"]}))
        .expect("request parses")
}

fn assert_lookup_key_unique(block: &Block) {
    let Shape::Table { key, columns, rows } = &block.shape else {
        panic!("{:?} is not a table", block.id);
    };
    assert_eq!(key, "lookup", "key column of {:?}", block.id);
    assert_eq!(
        columns.first().map(String::as_str),
        Some("lookup"),
        "first column"
    );
    assert!(rows.len() >= 3, "expected several rows, got {}", rows.len());
    let cells: Vec<String> = rows
        .iter()
        .map(|row| serde_json::to_string(&row[0]).unwrap())
        .collect();
    let mut sorted = cells.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), cells.len(), "duplicate key cells: {cells:?}");
}

fn has_latency(node: &Node) -> bool {
    node.fields.iter().any(|(k, _)| k == "latency")
}

#[tokio::test]
async fn compare_table_key_is_a_unique_lookup_column() {
    let result = testing::stub_jobs()
        .compare(&post_many(), client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = compare_blocks(&result);
    assert_lookup_key_unique(&blocks[0]);
}

#[tokio::test]
async fn authcompare_table_key_is_a_unique_lookup_column() {
    let result = testing::stub_jobs()
        .authcompare(&post_many(), client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = authcompare_blocks(&result);
    assert_lookup_key_unique(&blocks[0]);
}

#[test]
fn trace_server_whose_query_failed_has_no_latency() {
    let server = |ip: &str, outcome| ServerResult {
        server_ip: ip.parse().unwrap(),
        server_name: None,
        latency_ms: 0.0,
        outcome,
        answer_records: Vec::new(),
        referral_ns: Vec::new(),
        authority_zone: None,
    };
    let hop = TraceHop {
        level: 0,
        zone: ".".into(),
        servers_queried: 2,
        server_results: vec![
            server("198.51.100.1", ServerOutcome::Answer),
            server(
                "198.51.100.2",
                ServerOutcome::Error {
                    message: "timeout".into(),
                },
            ),
        ],
        referral_groups: Vec::new(),
        is_final: true,
    };
    let result = DnsTraceResult {
        hops: vec![DnsHopEvent {
            request_id: "rid".into(),
            hop,
        }],
        ..Default::default()
    };
    let blocks = trace_blocks(&result);
    let Shape::Tree(root) = &blocks[0].shape else {
        panic!("tree")
    };
    let servers = &root.children[0].children;
    assert_eq!(servers.len(), 2);
    assert!(has_latency(&servers[0]), "answering server keeps latency");
    assert!(
        !has_latency(&servers[1]),
        "failed server: {:?}",
        servers[1].fields
    );
}

#[test]
fn dnssec_level_with_no_servers_queried_has_no_latency() {
    let level = |n, queried| ChainLevel {
        level: n,
        zone: format!("z{n}.example.com."),
        servers_queried: queried,
        dnskey_records: Vec::new(),
        ds_records: Vec::new(),
        rrsig_records: Vec::new(),
        findings: Vec::new(),
        latency_ms: 0.0,
        is_final: false,
    };
    let result = DnsDnssecResult {
        levels: vec![
            DnsChainEvent {
                request_id: "rid".into(),
                level: level(0, 2),
            },
            DnsChainEvent {
                request_id: "rid".into(),
                level: level(1, 0),
            },
        ],
        ..Default::default()
    };
    let blocks = dnssec_blocks(&result);
    let Shape::Tree(root) = &blocks[0].shape else {
        panic!("tree")
    };
    assert!(
        has_latency(&root.children[0]),
        "queried level keeps latency"
    );
    assert!(
        !has_latency(&root.children[1]),
        "unqueried: {:?}",
        root.children[1].fields
    );
}
