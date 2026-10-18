// V2 Phase 7, operator decision 2026-10-18 (Rule M in the DNS inspector): every record type
// whose lookups all failed without an error event (SERVFAIL, timeout, refused) shows a visible
// "not measured" outcome with its reason; a partly failed query shows the answered types and the
// failed ones. Second-pass amendment: the authcompare table renders a record the same way in
// authoritative and recursive rows. Over `testing::stub_jobs()` (offline).

use std::net::IpAddr;

use netray_dns::blocks::{authcompare_blocks, query_blocks};
use netray_dns::config::Config;
use netray_dns::security::RateLimitState;
use netray_dns::{DnsQueryResult, testing};
use netray_model::evidence::{Block, Shape, Value};

fn client() -> IpAddr {
    "203.0.113.7".parse().unwrap()
}

fn limiter() -> RateLimitState {
    RateLimitState::new(&Config::load(None).expect("default config").limits)
}

async fn query(q: &str) -> DnsQueryResult {
    let parsed = netray_dns::parser::parse(q).expect("query parses");
    testing::stub_jobs()
        .query_parsed(parsed, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await
}

fn block<'a>(blocks: &'a [Block], name: &str) -> &'a Block {
    blocks
        .iter()
        .find(|b| b.id.name() == name)
        .unwrap_or_else(|| panic!("no dns.{name} in {blocks:?}"))
}

fn field<'a>(block: &'a Block, key: &str) -> &'a str {
    let Shape::Fields(fields) = &block.shape else {
        panic!("{:?} is not a fields block: {:?}", block.id, block.shape);
    };
    match fields.iter().find(|(k, _)| k == key) {
        Some((_, Value::Text(text))) => text,
        other => panic!("{:?} field {key}: {other:?}", block.id),
    }
}

#[tokio::test]
async fn a_servfail_type_shows_not_measured_with_its_reason() {
    let result = query("dnssec-failed.org A").await;
    let blocks = query_blocks(&result);

    let a = block(&blocks, "a");
    assert_eq!(field(a, "outcome"), "not measured");
    assert!(field(a, "reason").contains("SERVFAIL"), "{a:?}");
}

#[tokio::test]
async fn a_partly_failed_query_shows_the_answered_and_the_failed_types() {
    let result = query("partial.example.net A AAAA").await;
    let blocks = query_blocks(&result);

    let a = block(&blocks, "a");
    let Shape::Records(records) = &a.shape else {
        panic!("dns.a is not a records block: {a:?}");
    };
    assert_eq!(records.len(), 1, "{records:?}");

    let aaaa = block(&blocks, "aaaa");
    assert_eq!(field(aaaa, "outcome"), "not measured");
    assert!(field(aaaa, "reason").contains("SERVFAIL"), "{aaaa:?}");
}

#[tokio::test]
async fn authcompare_renders_txt_the_same_in_every_row() {
    let parsed = netray_dns::parser::parse("example.com TXT").expect("query parses");
    let result = testing::stub_jobs()
        .authcompare_parsed(parsed, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = authcompare_blocks(&result);
    let Shape::Table { columns, rows, .. } = &blocks[0].shape else {
        panic!("dns.authcompare is not a table");
    };
    let col = |name: &str| columns.iter().position(|c| c == name).expect(name);
    let (source, records) = (col("source"), col("records"));
    let mut sources = std::collections::BTreeSet::new();
    let mut texts = std::collections::BTreeSet::new();
    for row in rows {
        sources.insert(format!("{:?}", row[source]));
        texts.insert(format!("{:?}", row[records]));
    }
    assert!(sources.len() >= 2, "both sources expected: {rows:?}");
    assert_eq!(texts.len(), 1, "TXT rendered differently: {texts:?}");
}
