//! TLS rebuilds its section from its own evidence (specs/features/v2-fixture-consistency/spec.md,
//! requirement 2 and 6; criteria C2, C4, C9 to C13).
//!
//! `netray_tls::testing::from_evidence` takes the `tls.*` blocks of a stored run plus the facts
//! blocks `dns.caa` and `dns.https`, and runs the module's own assessment. The base input is the
//! seeded shop fixture: four addresses, three-item chains ending self-signed.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::CheckId;
use netray_model::Status;
use netray_model::evidence::{Block, Shape, Value};
use netray_model::run::Run;

const FIXTURE: &str = "checkout.eu-central.storefront.example.com.json";

fn load() -> (String, Vec<Block>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history")
        .join(FIXTURE);
    let text = std::fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let domain = v["domain"].as_str().unwrap().to_string();
    let run: Run = serde_json::from_value(v["run"].clone()).unwrap();
    let blocks = run
        .blocks
        .into_iter()
        .filter(|b| {
            let id = b.id.to_string();
            id.starts_with("tls.") || id == "dns.caa" || id == "dns.https"
        })
        .collect();
    (domain, blocks)
}

fn is_chain(b: &Block) -> bool {
    b.id.to_string().starts_with("tls.chain@")
}

/// Set `key` on the leaf (first item) of every chain block, or of those whose id contains `only`.
fn set_leaf(blocks: &mut [Block], only: Option<&str>, key: &str, value: Value) {
    for b in blocks.iter_mut().filter(|b| is_chain(b)) {
        if only.is_some_and(|o| !b.id.to_string().contains(o)) {
            continue;
        }
        if let Shape::Chain(items) = &mut b.shape {
            for f in items[0].iter_mut().filter(|(k, _)| k == key) {
                f.1 = value.clone();
            }
        }
    }
}

fn status(outcome: &SectionOutcome, id: &str) -> Status {
    let SectionOutcome::Measured { checks, .. } = outcome else {
        panic!("not measured: {outcome:?}");
    };
    let id = CheckId::parse(id).unwrap();
    checks.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("no check {id}")).status
}

async fn run(domain: &str, blocks: &[Block]) -> SectionOutcome {
    netray_tls::testing::from_evidence(domain, blocks).await
}

#[tokio::test]
async fn measured_with_all_23_checks() {
    let (domain, blocks) = load();
    let SectionOutcome::Measured { checks, .. } = run(&domain, &blocks).await else {
        panic!("not measured");
    };
    assert_eq!(checks.len(), 23);
}

#[tokio::test]
async fn expiry_window_follows_days_remaining() {
    let (domain, mut blocks) = load();
    set_leaf(&mut blocks, None, "days_remaining", Value::Int(10));
    assert_eq!(status(&run(&domain, &blocks).await, "tls.expiry_window"), Status::Warn);
    set_leaf(&mut blocks, None, "days_remaining", Value::Int(68));
    assert_eq!(status(&run(&domain, &blocks).await, "tls.expiry_window"), Status::Pass);
}

#[tokio::test]
async fn differing_leaf_fingerprints_break_consistency() {
    let (domain, mut blocks) = load();
    assert_eq!(status(&run(&domain, &blocks).await, "tls.consistency"), Status::Pass);
    let other = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";
    set_leaf(&mut blocks, Some("2001:db8::11"), "fingerprint_sha256", Value::Text(other.into()));
    assert_ne!(status(&run(&domain, &blocks).await, "tls.consistency"), Status::Pass);
}

#[tokio::test]
async fn self_signed_end_is_trusted_and_foreign_sans_fail_hostname() {
    let (domain, mut blocks) = load();
    let out = run(&domain, &blocks).await;
    assert_eq!(status(&out, "tls.chain_trusted"), Status::Pass);
    assert_eq!(status(&out, "tls.chain_complete"), Status::Pass);
    assert_eq!(status(&out, "tls.hostname_match"), Status::Pass);
    for b in blocks.iter_mut().filter(|b| is_chain(b)) {
        if let Shape::Chain(items) = &mut b.shape {
            items[0].retain(|(k, _)| k != "sans");
            items[0].push(("sans".into(), Value::Text("other.example.net".into())));
        }
    }
    assert_eq!(status(&run(&domain, &blocks).await, "tls.hostname_match"), Status::Fail);
}

#[tokio::test]
async fn expired_key_reads_the_run_time_not_the_wall_clock() {
    let (domain, mut blocks) = load();
    set_leaf(&mut blocks, None, "not_before", Value::Time("2019-10-01T00:00:00Z".into()));
    set_leaf(&mut blocks, None, "not_after", Value::Time("2020-01-01T00:00:00Z".into()));
    set_leaf(&mut blocks, None, "days_remaining", Value::Int(30));
    let SectionOutcome::Measured { evidence, .. } = run(&domain, &blocks).await else {
        panic!("not measured");
    };
    let mut seen = 0;
    for b in evidence.iter().filter(|b| is_chain(b)) {
        let Shape::Chain(items) = &b.shape else { panic!("chain shape") };
        for item in items {
            for (k, v) in item.iter().filter(|(k, _)| k == "expired") {
                assert_eq!(v, &Value::Bool(false), "{} {k}", b.id);
                seen += 1;
            }
        }
    }
    assert!(seen > 0, "no expired key in the returned chains");
}

#[tokio::test]
async fn rebuild_is_a_fixpoint() {
    let (domain, blocks) = load();
    let first = run(&domain, &blocks).await;
    let SectionOutcome::Measured { evidence, .. } = &first else {
        panic!("not measured");
    };
    let mut again: Vec<Block> = evidence.clone();
    again.extend(blocks.iter().filter(|b| {
        let id = b.id.to_string();
        id == "dns.caa" || id == "dns.https"
    }).cloned());
    assert_eq!(run(&domain, &again).await, first);
}
