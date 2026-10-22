//! IP `from_evidence` rebuilds the section from its own blocks
//! (specs/features/v2-fixture-consistency/spec.md, requirement 5 and 6; criteria C3, C4, C14, C15).
//!
//! The base input is `new.example.com`'s fixture: four `ip.addresses` rows, two A and two AAAA
//! records.
//!
//! Copied from crates/lens/tests/it/fixture_evidence_ip.rs (spec v2-cutover,
//! Phase 6); nothing dropped.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::Status;
use netray_model::evidence::{Block, Shape, Value};
use netray_model::run::Run;
use serde::Deserialize;

const DOMAIN: &str = "new.example.com";

#[derive(Deserialize)]
struct Fixture {
    run: Run,
}

/// The fixture's `ip.addresses`, `dns.a` and `dns.aaaa` blocks.
fn base_blocks() -> Vec<Block> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/new.example.com.json");
    let fixture: Fixture = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let blocks: Vec<Block> = fixture
        .run
        .blocks
        .into_iter()
        .filter(|b| ["ip.addresses", "dns.a", "dns.aaaa"].contains(&b.id.to_string().as_str()))
        .collect();
    assert_eq!(blocks.len(), 3);
    blocks
}

fn block_mut<'a>(blocks: &'a mut [Block], id: &str) -> &'a mut Block {
    blocks.iter_mut().find(|b| b.id.to_string() == id).unwrap()
}

fn measured(outcome: SectionOutcome) -> (Vec<netray_model::CheckResult>, Vec<Block>) {
    match outcome {
        SectionOutcome::Measured { checks, evidence } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    }
}

#[tokio::test]
async fn ip_from_evidence_counts_checked_addresses_against_the_records() {
    let mut blocks = base_blocks();

    let (checks, evidence) = measured(netray_ip::testing::from_evidence(DOMAIN, &blocks).await);
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].id.to_string(), "ip.reputation");
    assert_eq!(checks[0].status, Status::Pass);
    assert!(checks[0].findings.is_empty(), "{:?}", checks[0].findings);
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].id.to_string(), "ip.addresses");
    match &evidence[0].shape {
        Shape::Table { rows, .. } => assert_eq!(rows.len(), 4),
        other => panic!("expected a table, got {other:?}"),
    }

    // A fifth A record the rows do not cover.
    match &mut block_mut(&mut blocks, "dns.a").shape {
        Shape::Records(records) => {
            let mut extra = records[0].clone();
            extra.data = Value::Text("203.0.113.10".into());
            records.push(extra);
        }
        other => panic!("expected records, got {other:?}"),
    }
    let (checks, _) = measured(netray_ip::testing::from_evidence(DOMAIN, &blocks).await);
    assert!(
        checks[0]
            .findings
            .contains(&"checked 4 of 5 addresses".to_string()),
        "{:?}",
        checks[0].findings
    );
}

#[tokio::test]
async fn ip_from_evidence_fails_reputation_on_a_tor_row_and_passes_without_flags() {
    let mut blocks = base_blocks();

    // Column 9 is `tor`.
    match &mut block_mut(&mut blocks, "ip.addresses").shape {
        Shape::Table { columns, rows, .. } => {
            assert_eq!(columns[9], "tor");
            rows[0][9] = Value::Bool(true);
        }
        other => panic!("expected a table, got {other:?}"),
    }
    let (checks, _) = measured(netray_ip::testing::from_evidence(DOMAIN, &blocks).await);
    assert_eq!(checks[0].status, Status::Fail);
    assert_eq!(checks[0].findings, vec!["192.0.2.10: tor".to_string()]);

    match &mut block_mut(&mut blocks, "ip.addresses").shape {
        Shape::Table { rows, .. } => rows[0][9] = Value::Bool(false),
        other => panic!("expected a table, got {other:?}"),
    }
    let (checks, _) = measured(netray_ip::testing::from_evidence(DOMAIN, &blocks).await);
    assert_eq!(checks[0].status, Status::Pass);
}

#[tokio::test]
async fn ip_from_evidence_is_a_fixpoint() {
    let base = base_blocks();
    let first = netray_ip::testing::from_evidence(DOMAIN, &base).await;

    let mut again: Vec<Block> = match &first {
        SectionOutcome::Measured { evidence, .. } => evidence.clone(),
        other => panic!("expected Measured, got {other:?}"),
    };
    again.extend(
        base.into_iter()
            .filter(|b| b.id.to_string() != "ip.addresses"),
    );

    let second = netray_ip::testing::from_evidence(DOMAIN, &again).await;
    assert_eq!(first, second);
}
