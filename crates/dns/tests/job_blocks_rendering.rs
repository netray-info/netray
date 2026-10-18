// V2 Phase 7, third review: every record type the inspector offers renders in zone-file form,
// authoritative and recursive authcompare rows render a record the same way, and an
// authoritative REFUSED is a failed lookup, never "no records" (Rule M). Over
// `testing::stub_jobs()` (offline).

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;

use netray_dns::blocks::{authcompare_blocks, query_blocks};
use netray_dns::config::Config;
use netray_dns::security::RateLimitState;
use netray_dns::testing;
use netray_model::evidence::{Shape, Value};

fn client() -> IpAddr {
    "203.0.113.7".parse().unwrap()
}

fn limiter() -> RateLimitState {
    RateLimitState::new(&Config::load(None).expect("default config").limits)
}

fn text(value: &Value) -> String {
    match value {
        Value::Text(t) => t.clone(),
        other => panic!("not text: {other:?}"),
    }
}

/// (type, records, outcome) of every authcompare row of `q`, with its source.
async fn authcompare_rows(q: &str) -> Vec<(String, String, String, String)> {
    let parsed = netray_dns::parser::parse(q).expect("query parses");
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
    rows.iter()
        .map(|row| {
            (
                text(&row[col("source")]),
                text(&row[col("type")]),
                text(&row[col("records")]),
                text(&row[col("outcome")]),
            )
        })
        .collect()
}

#[tokio::test]
async fn every_offered_type_renders_in_zone_file_form() {
    let parsed = netray_dns::parser::parse("types.example.com SRV TLSA SSHFP NAPTR HINFO")
        .expect("query parses");
    let result = testing::stub_jobs()
        .query_parsed(parsed, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = query_blocks(&result);
    let data: BTreeMap<String, String> = blocks
        .iter()
        .map(|b| {
            let Shape::Records(records) = &b.shape else {
                panic!("{:?} is not records: {:?}", b.id, b.shape);
            };
            (b.title.clone(), text(&records[0].data))
        })
        .collect();
    assert_eq!(data["SRV"], "10 5 5060 sip.example.com.");
    assert_eq!(data["TLSA"], "3 1 1 ABCD01");
    assert_eq!(data["SSHFP"], "4 2 12EF");
    assert_eq!(
        data["NAPTR"],
        "100 10 \"S\" \"SIP+D2U\" \"\" _sip._udp.example.com."
    );
    assert_eq!(data["HINFO"], "\"INTEL\" \"LINUX\"");
}

#[tokio::test]
async fn authcompare_renders_every_type_the_same_in_every_row() {
    let mut rows = authcompare_rows("example.com A MX TXT NS CAA").await;
    rows.extend(authcompare_rows("types.example.com SRV TLSA SSHFP NAPTR HINFO").await);
    let mut by_type: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut sources = BTreeSet::new();
    for (source, record_type, records, _) in rows {
        sources.insert(source);
        by_type.entry(record_type).or_default().insert(records);
    }
    assert!(sources.len() >= 2, "both sources expected");
    for (record_type, texts) in by_type {
        assert_eq!(
            texts.len(),
            1,
            "{record_type} rendered differently: {texts:?}"
        );
    }
}

#[tokio::test]
async fn an_authoritative_refused_is_not_no_records() {
    let rows = authcompare_rows("lame.example.com A").await;
    let authoritative: Vec<_> = rows
        .iter()
        .filter(|(source, ..)| source != "recursive")
        .collect();
    assert!(!authoritative.is_empty(), "no authoritative rows: {rows:?}");
    for (_, _, records, outcome) in authoritative {
        assert_ne!(outcome, "no records", "{rows:?}");
        assert!(
            outcome.contains("REFUSED") || outcome.contains("Refused"),
            "{rows:?}"
        );
        assert_eq!(records, outcome, "{rows:?}");
    }
}
