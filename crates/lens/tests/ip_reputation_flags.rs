// Contract: lens scores an address from ifconfig-rs's booleans. `is_spamhaus`, `is_c2`,
// `is_tor` -> Fail; `is_vpn` -> Warn; otherwise Pass. `network.type` is display only.
// (grade-integrity, requirement 3: C1, C3.) The section comes from `netray_ip`'s
// `golden_module` on the golden with the flags set, through the registry.
//
// Dropped: C4 "a body without the boolean flags is not Pass". In-process the lookup is a typed
// `Ifconfig`, so a body without flags is not a lookup result at all; the typed decode of the
// golden is the contract (`crates/ip/src/contract_golden.rs`).

mod common;

use std::time::Duration;

use common::{ip_with_body, run_ip};
use lens::scoring::engine::CheckVerdict;

const GOLDEN: &str = include_str!("../../../tests/fixtures/contracts/ifconfig-json.json");

/// The golden with the given `network.<key>` values set.
fn golden_with(sets: &[(&str, serde_json::Value)]) -> String {
    let mut v: serde_json::Value = serde_json::from_str(GOLDEN).unwrap();
    for (k, val) in sets {
        v["network"][*k] = val.clone();
    }
    v.to_string()
}

async fn reputation(body: String) -> CheckVerdict {
    let r = run_ip(ip_with_body(&body), Duration::from_secs(5), &["1.1.1.1"])
        .await
        .expect("ip section");
    r.checks
        .iter()
        .find(|c| c.name == "reputation")
        .expect("reputation check")
        .verdict
        .clone()
}

#[tokio::test]
async fn reputation_is_scored_from_ifconfig_flags() {
    use serde_json::json;
    let t = json!(true);
    let rows: Vec<(&str, String, CheckVerdict)> = vec![
        ("golden unchanged", golden_with(&[]), CheckVerdict::Pass),
        (
            "is_spamhaus",
            golden_with(&[("is_spamhaus", t.clone())]),
            CheckVerdict::Fail,
        ),
        (
            "is_c2",
            golden_with(&[("is_c2", t.clone())]),
            CheckVerdict::Fail,
        ),
        (
            "is_tor",
            golden_with(&[("is_tor", t.clone())]),
            CheckVerdict::Fail,
        ),
        (
            "is_vpn",
            golden_with(&[("is_vpn", t.clone())]),
            CheckVerdict::Warn,
        ),
        // type is display only: "tor" with every flag false is Pass
        (
            "type tor, no flags",
            golden_with(&[("type", json!("tor"))]),
            CheckVerdict::Pass,
        ),
    ];

    let mut failures = Vec::new();
    for (name, body, want) in rows {
        let got = reputation(body).await;
        if got != want {
            failures.push(format!("{name}: want {want:?}, got {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
