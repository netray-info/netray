// v2-dns-ui Phase 1 (C1, C8): the page blocks of a DNS job lead with the `dns.answers` and
// `dns.servers` trees, then carry the job's own blocks. Offline, over `testing::stub_jobs()`.

use std::net::IpAddr;

use netray_dns::config::Config;
use netray_dns::security::RateLimitState;
use netray_dns::{
    DnsCachedJob, PostQueryRequest, authcompare_page_blocks, compare_page_blocks,
    query_page_blocks, testing,
};
use netray_model::evidence::{Block, Node, Shape, Value};
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

fn tree<'a>(blocks: &'a [Block], name: &str) -> &'a Node {
    let b = blocks
        .iter()
        .find(|b| b.id.name() == name)
        .unwrap_or_else(|| panic!("no dns.{name} in {:?}", ids(blocks)));
    let Shape::Tree(node) = &b.shape else {
        panic!("dns.{name} is not a tree");
    };
    node
}

fn field<'a>(node: &'a Node, key: &str) -> Option<&'a Value> {
    node.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn text(node: &Node, key: &str) -> String {
    match field(node, key) {
        Some(Value::Text(t)) => t.clone(),
        other => panic!("{key}: {other:?}"),
    }
}

/// No empty text and no empty-string placeholder anywhere in the tree.
fn assert_no_placeholders(node: &Node) {
    for (k, v) in &node.fields {
        if let Value::Text(t) = v {
            assert!(!t.is_empty(), "empty text at {k}");
        }
    }
    node.children.iter().for_each(assert_no_placeholders);
}

fn assert_leads_with_trees(blocks: &[Block], own: &[&str]) {
    let got = ids(blocks);
    let mut want = vec!["dns.answers".to_string(), "dns.servers".to_string()];
    want.extend(own.iter().map(|s| s.to_string()));
    assert_eq!(got, want);
    assert!(matches!(blocks[0].shape, Shape::Tree(_)));
    assert!(matches!(blocks[1].shape, Shape::Tree(_)));
}

#[tokio::test]
async fn query_page_blocks_lead_with_answers_and_servers() {
    let parsed = netray_dns::parser::parse("example.com A").expect("query parses");
    let result = testing::stub_jobs()
        .query_parsed(parsed, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = query_page_blocks(&result);
    assert_leads_with_trees(&blocks, &["dns.a"]);

    let answers = tree(&blocks, "answers");
    assert_eq!(text(answers, "mode"), "query");
    assert_eq!(answers.children.len(), 1);
    assert_eq!(text(&answers.children[0], "record_type"), "A");
    assert_eq!(text(&answers.children[0], "agreement"), "agree");
    assert_no_placeholders(answers);
    assert_no_placeholders(tree(&blocks, "servers"));

    // The cached result carries the same blocks.
    let cached = DnsCachedJob::from(result);
    assert_eq!(cached.page_blocks(), blocks);
}

#[tokio::test]
async fn compare_page_blocks_lead_with_answers_and_servers() {
    let result = testing::stub_jobs()
        .compare(&post(), client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = compare_page_blocks(&result);
    assert_leads_with_trees(&blocks, &["dns.compare"]);
    assert_eq!(text(tree(&blocks, "answers"), "mode"), "compare");
    assert_eq!(DnsCachedJob::from(result).page_blocks(), blocks);
}

#[tokio::test]
async fn authcompare_page_blocks_lead_with_answers_and_servers() {
    let result = testing::stub_jobs()
        .authcompare(&post(), client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = authcompare_page_blocks(&result);
    assert_leads_with_trees(&blocks, &["dns.authcompare"]);
    assert_eq!(text(tree(&blocks, "answers"), "mode"), "authcompare");
    assert_eq!(DnsCachedJob::from(result).page_blocks(), blocks);
}
