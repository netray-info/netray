//! Email `from_evidence` rebuilds the module's outcome from the `email.*` blocks
//! (specs/features/v2-fixture-consistency/spec.md, Phase 2, R4 and R6; report.md C2, C3, C8, C9,
//! C10, C11). The base blocks are the `email.*` blocks of the seeded history fixture
//! `new.example.com`; each scenario edits the DMARC, DKIM or MTA-STS blocks.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::evidence::Block;
use netray_model::{CheckResult, Status};
use serde_json::{Value, json};

const DOMAIN: &str = "new.example.com";

const CHECK_IDS: [&str; 10] = [
    "email.mx",
    "email.spf",
    "email.dkim",
    "email.dmarc",
    "email.mta_sts",
    "email.tls_rpt",
    "email.bimi",
    "email.dane",
    "email.dnsbl",
    "email.fcrdns",
];

/// The fixture's `email.*` blocks as JSON, so a scenario edits them without the shape types.
fn base_blocks() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/new.example.com.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    fixture["run"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["id"].as_str().unwrap().starts_with("email."))
        .cloned()
        .collect()
}

fn block_mut<'a>(blocks: &'a mut [Value], id: &str) -> &'a mut Value {
    blocks.iter_mut().find(|b| b["id"] == id).unwrap()
}

/// Sets the field `name` of a `fields` block, replacing a pair of that name.
fn set_field(blocks: &mut [Value], id: &str, name: &str, value: Value) {
    let data = block_mut(blocks, id)["shape"]["data"]
        .as_array_mut()
        .unwrap();
    data.retain(|pair| pair[0] != name);
    data.push(json!([name, value]));
}

fn text(v: &str) -> Value {
    json!({"t": "text", "v": v})
}

fn parse(blocks: Vec<Value>) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|b| serde_json::from_value(b).unwrap())
        .collect()
}

async fn outcome(blocks: Vec<Value>) -> SectionOutcome {
    netray_email::testing::from_evidence(DOMAIN, &parse(blocks)).await
}

fn measured(outcome: SectionOutcome) -> (Vec<CheckResult>, Vec<Block>) {
    match outcome {
        SectionOutcome::Measured { checks, evidence } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    }
}

async fn check_of(blocks: Vec<Value>, check: &str) -> CheckResult {
    let (checks, _) = measured(outcome(blocks).await);
    checks
        .into_iter()
        .find(|c| c.id.to_string() == check)
        .unwrap_or_else(|| panic!("no check {check}"))
}

#[tokio::test]
async fn email_from_evidence_measures_all_ten_checks() {
    let (checks, _) = measured(outcome(base_blocks()).await);
    let ids: Vec<String> = checks.iter().map(|c| c.id.to_string()).collect();
    for id in CHECK_IDS {
        assert!(ids.iter().any(|i| i == id), "missing {id} in {ids:?}");
    }
}

#[tokio::test]
async fn email_from_evidence_dmarc_follows_the_policy() {
    let reject = check_of(base_blocks(), "email.dmarc").await;
    assert!(
        !(reject.status == Status::Warn && reject.findings.iter().any(|f| f.contains("p=none"))),
        "{reject:?}"
    );

    let mut none = base_blocks();
    set_field(&mut none, "email.dmarc", "record", text("v=DMARC1; p=none"));
    set_field(&mut none, "email.dmarc", "p", text("none"));
    let none = check_of(none, "email.dmarc").await;
    assert_eq!(none.status, Status::Warn, "{none:?}");
}

#[tokio::test]
async fn email_from_evidence_dkim_follows_the_key_rows() {
    let mut without = base_blocks();
    block_mut(&mut without, "email.dkim")["shape"]["data"]["rows"] = json!([]);
    assert_ne!(check_of(without, "email.dkim").await.status, Status::Pass);

    let with = check_of(base_blocks(), "email.dkim").await;
    assert_eq!(with.status, Status::Pass, "{with:?}");
}

#[tokio::test]
async fn email_from_evidence_mta_sts_follows_the_recorded_policy() {
    let blocks = base_blocks();
    let mx = blocks.iter().find(|b| b["id"] == "email.mx").unwrap()["shape"]["data"]["rows"][0][0]
        ["v"]
        .as_str()
        .unwrap()
        .trim_end_matches('.')
        .to_string();
    assert_eq!(mx, "mx1.new.example.com");

    let policy = |mode: &str| {
        let mut b = base_blocks();
        block_mut(&mut b, "email.mta_sts_policy")["shape"]["data"] = json!(format!(
            "version: STSv1\nmode: {mode}\nmx: {mx}\nmax_age: 604800\n"
        ));
        b
    };
    let enforce = check_of(policy("enforce"), "email.mta_sts").await;
    assert_eq!(enforce.status, Status::Pass, "{enforce:?}");
    let testing = check_of(policy("testing"), "email.mta_sts").await;
    assert_eq!(testing.status, Status::Warn, "{testing:?}");
}

#[tokio::test]
async fn email_from_evidence_is_a_fixpoint() {
    let first = outcome(base_blocks()).await;
    let (_, evidence) = measured(first.clone());
    let email: Vec<Block> = evidence
        .into_iter()
        .filter(|b| b.id.to_string().starts_with("email."))
        .collect();
    let second = netray_email::testing::from_evidence(DOMAIN, &email).await;
    assert_eq!(first, second);
}
