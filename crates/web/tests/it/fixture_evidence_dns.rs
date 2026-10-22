//! DNS `from_evidence` rebuilds the module's outcome from the `dns.*` blocks
//! (specs/features/v2-fixture-consistency/spec.md, Phase 2; report.md C1, C3, C4, C5, C6, C7).
//! The base blocks are the `dns.*` blocks of the seeded history fixture `new.example.com`;
//! fixtures carry no RRSIG, so an empty `dns.rrsig` block stands for a definite NODATA.
//!
//! Copied from crates/lens/tests/it/fixture_evidence_dns.rs (spec v2-cutover,
//! Phase 6); nothing dropped.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::evidence::Block;
use netray_model::{CheckResult, Status};
use serde_json::{Value, json};

const DOMAIN: &str = "new.example.com";

const CHECK_IDS: [&str; 10] = [
    "dns.caa",
    "dns.cname_apex",
    "dns.dnssec",
    "dns.dnskey_algorithm",
    "dns.dnssec_rollover",
    "dns.https_svcb",
    "dns.ns",
    "dns.ns_lame",
    "dns.ns_delegation",
    "dns.ttl",
];

/// A KSK of the fixture zone (flags 257, protocol 3, algorithm 13).
const KSK_KEY: &str =
    "mdsswUyr3DPW132mOi8V9xESWE8jTo0dxCjjnopKl+GqJxpVXckHAeF+KkxLbxILfDLUT0rAK9iUzy1L53eKGQ==";

/// The fixture's `dns.*` blocks as JSON, plus an empty `dns.rrsig` block when absent.
fn base_blocks() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/new.example.com.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut blocks: Vec<Value> = fixture["run"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["id"].as_str().unwrap().starts_with("dns."))
        .cloned()
        .collect();
    if !blocks.iter().any(|b| b["id"] == "dns.rrsig") {
        blocks.push(json!({
            "id": "dns.rrsig",
            "title": "RRSIG",
            "shape": {"kind": "records", "data": []}
        }));
    }
    blocks
}

fn record(rtype: &str, ttl: i64, data: &str) -> Value {
    json!({
        "owner": {"t": "text", "v": "new.example.com."},
        "type": {"t": "text", "v": rtype},
        "ttl": {"t": "int", "v": ttl},
        "data": {"t": "text", "v": data}
    })
}

fn set_records(blocks: &mut [Value], id: &str, rows: Vec<Value>) {
    let block = blocks.iter_mut().find(|b| b["id"] == id).unwrap();
    block["shape"]["data"] = Value::Array(rows);
}

/// Standard base64 decoding, padding optional.
fn base64_decode(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("bad base64"),
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// RFC 4034 Appendix B key tag over the DNSKEY rdata.
fn key_tag(flags: u16, algorithm: u8, key: &str) -> u16 {
    let mut rdata = flags.to_be_bytes().to_vec();
    rdata.extend([3, algorithm]);
    rdata.extend(base64_decode(key));
    let mut acc: u32 = 0;
    for (i, b) in rdata.iter().enumerate() {
        acc += if i % 2 == 0 {
            u32::from(*b) << 8
        } else {
            u32::from(*b)
        };
    }
    acc += (acc >> 16) & 0xffff;
    (acc & 0xffff) as u16
}

const DS_DIGEST: &str = "C988EC423E3880EB8DD8A46FE06CA230EE23F35B578D64E78B29C3E1C83D245A";

/// One KSK in `dns.dnskey` and one DS pointing at `ds_tag`.
fn with_ds(ds_tag: u16) -> Vec<Value> {
    let mut blocks = base_blocks();
    set_records(
        &mut blocks,
        "dns.dnskey",
        vec![record("DNSKEY", 1722, &format!("257 3 13 {KSK_KEY}"))],
    );
    set_records(
        &mut blocks,
        "dns.ds",
        vec![record("DS", 84873, &format!("{ds_tag} 13 2 {DS_DIGEST}"))],
    );
    blocks
}

fn parse(blocks: Vec<Value>) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|b| serde_json::from_value(b).unwrap())
        .collect()
}

async fn outcome(blocks: Vec<Value>) -> SectionOutcome {
    netray_dns::testing::from_evidence(DOMAIN, &parse(blocks)).await
}

fn measured(outcome: SectionOutcome) -> (Vec<CheckResult>, Vec<Block>) {
    match outcome {
        SectionOutcome::Measured { checks, evidence } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    }
}

async fn status_of(blocks: Vec<Value>, check: &str) -> Status {
    let (checks, _) = measured(outcome(blocks).await);
    checks
        .iter()
        .find(|c| c.id.to_string() == check)
        .unwrap_or_else(|| panic!("no check {check}"))
        .status
}

#[tokio::test]
async fn dns_from_evidence_measures_all_ten_checks() {
    let (checks, _) = measured(outcome(base_blocks()).await);
    let ids: Vec<String> = checks.iter().map(|c| c.id.to_string()).collect();
    for id in CHECK_IDS {
        assert!(ids.iter().any(|i| i == id), "missing {id} in {ids:?}");
    }
    assert_eq!(ids.len(), 10, "{ids:?}");
}

#[tokio::test]
async fn dns_from_evidence_caa_follows_the_caa_records() {
    let mut empty = base_blocks();
    set_records(&mut empty, "dns.caa", vec![]);
    assert_eq!(status_of(empty, "dns.caa").await, Status::Warn);

    let mut present = base_blocks();
    set_records(
        &mut present,
        "dns.caa",
        vec![
            record("CAA", 3600, "0 issue \"letsencrypt.org\""),
            record("CAA", 3600, "0 issuewild \";\""),
        ],
    );
    assert_eq!(status_of(present, "dns.caa").await, Status::Pass);
}

#[tokio::test]
async fn dns_from_evidence_rollover_follows_the_ds_key_tag() {
    let tag = key_tag(257, 13, KSK_KEY);
    assert_eq!(tag, 2371, "the helper reproduces the fixture's tag");
    assert_eq!(
        status_of(with_ds(tag), "dns.dnssec_rollover").await,
        Status::Pass
    );
    assert_eq!(
        status_of(with_ds(tag.wrapping_add(1)), "dns.dnssec_rollover").await,
        Status::Fail
    );
}

#[tokio::test]
async fn dns_from_evidence_ns_follows_the_ns_record_count() {
    let mut one = base_blocks();
    set_records(
        &mut one,
        "dns.ns",
        vec![record("NS", 77532, "ns1.new.example.com.")],
    );
    assert_eq!(status_of(one, "dns.ns").await, Status::Warn);

    let mut two = base_blocks();
    set_records(
        &mut two,
        "dns.ns",
        vec![
            record("NS", 77532, "ns1.new.example.com."),
            record("NS", 77532, "ns2.new.example.com."),
        ],
    );
    assert_eq!(status_of(two, "dns.ns").await, Status::Pass);
}

#[tokio::test]
async fn dns_from_evidence_is_a_fixpoint() {
    let first = outcome(base_blocks()).await;
    let (_, evidence) = measured(first.clone());
    let dns_blocks: Vec<Block> = evidence
        .into_iter()
        .filter(|b| b.id.to_string().starts_with("dns."))
        .collect();
    let second = netray_dns::testing::from_evidence(DOMAIN, &dns_blocks).await;
    assert_eq!(first, second);
}
