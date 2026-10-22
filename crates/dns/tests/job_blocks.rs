// V2 Phase 7, requirement 18: each collected DNS job maps to evidence blocks
// (`netray_dns::blocks`), over `testing::stub_jobs()` (offline: fixed zone for example.com).

use std::net::IpAddr;

use netray_dns::blocks::{
    authcompare_blocks, compare_blocks, dnssec_blocks, query_blocks, trace_blocks,
};
use netray_dns::config::Config;
use netray_dns::security::RateLimitState;
use netray_dns::{DnssecRequest, PostQueryRequest, TraceRequest, testing};
use netray_model::evidence::{Block, Shape};
use serde_json::json;

fn client() -> IpAddr {
    "203.0.113.7".parse().unwrap()
}

fn limiter() -> RateLimitState {
    RateLimitState::new(&Config::default().limits)
}

fn post() -> PostQueryRequest {
    serde_json::from_value(json!({"domain": "example.com", "record_types": ["A"]}))
        .expect("request parses")
}

fn ids(blocks: &[Block]) -> Vec<String> {
    blocks
        .iter()
        .map(|b| format!("{}.{}", b.id.module(), b.id.name()))
        .collect()
}

fn assert_all_dns(blocks: &[Block]) {
    for b in blocks {
        assert_eq!(b.id.module(), "dns", "block {:?}", b.id);
    }
}

#[tokio::test]
async fn query_a_and_mx_become_records_blocks() {
    let jobs = testing::stub_jobs();
    let parsed = netray_dns::parser::parse("example.com A MX").expect("query parses");
    let result = jobs
        .query_parsed(parsed, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = query_blocks(&result);
    assert_all_dns(&blocks);
    let got = ids(&blocks);
    assert!(got.contains(&"dns.a".to_string()), "ids: {got:?}");
    assert!(got.contains(&"dns.mx".to_string()), "ids: {got:?}");
    for b in &blocks {
        assert!(
            matches!(b.shape, Shape::Records(_)),
            "{:?} is records",
            b.id
        );
    }
    let a = blocks.iter().find(|b| b.id.name() == "a").unwrap();
    let Shape::Records(records) = &a.shape else {
        unreachable!()
    };
    let text = serde_json::to_string(&records).unwrap();
    assert!(text.contains("192.0.2.10"), "A block: {text}");
}

#[tokio::test]
async fn compare_becomes_one_table_block() {
    let result = testing::stub_jobs()
        .compare(&post(), client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = compare_blocks(&result);
    assert_eq!(ids(&blocks), vec!["dns.compare"]);
    assert!(matches!(blocks[0].shape, Shape::Table { .. }));
}

#[tokio::test]
async fn authcompare_becomes_one_table_block() {
    let result = testing::stub_jobs()
        .authcompare(&post(), client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = authcompare_blocks(&result);
    assert_eq!(ids(&blocks), vec!["dns.authcompare"]);
    assert!(matches!(blocks[0].shape, Shape::Table { .. }));
}

#[tokio::test]
async fn trace_becomes_one_tree_block() {
    let req: TraceRequest =
        serde_json::from_value(json!({"domain": "example.com"})).expect("trace request parses");
    let result = testing::stub_jobs()
        .trace(&req, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = trace_blocks(&result);
    assert_eq!(ids(&blocks), vec!["dns.trace"]);
    assert!(matches!(blocks[0].shape, Shape::Tree(_)));
}

#[tokio::test]
async fn dnssec_becomes_one_tree_block() {
    let req: DnssecRequest =
        serde_json::from_value(json!({"domain": "example.com"})).expect("dnssec request parses");
    let result = testing::stub_jobs()
        .dnssec(&req, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = dnssec_blocks(&result);
    assert_eq!(ids(&blocks), vec!["dns.dnssec_chain"]);
    assert!(matches!(blocks[0].shape, Shape::Tree(_)));
}
