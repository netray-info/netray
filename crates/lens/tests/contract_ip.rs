// Contract: lens must understand the real ifconfig-rs `/json?ip=` body.
// The golden is written by `cargo test -p netray-ip --lib contract_golden`
// (UPDATE_GOLDEN=1 regenerates it). The IP section comes from `netray_ip`'s `golden_module`
// through the registry; the address reaches it from the DNS section's result.

mod common;

use std::time::Duration;

use common::{golden, ip_golden, run_ip};
use lens::modules::BackendExtra;

#[tokio::test]
async fn ip_module_reads_real_ifconfig_json_body() {
    let body: serde_json::Value = serde_json::from_str(&golden("ifconfig-json.json")).unwrap();
    let want_type = body["network"]["type"].as_str().unwrap().to_string();
    let want_org = body["network"]["org"].as_str().unwrap().to_string();
    let want_geo = format!(
        "{}, {}",
        body["location"]["city"].as_str().unwrap(),
        body["location"]["country"].as_str().unwrap()
    );
    assert_ne!(
        want_type, "residential",
        "golden must carry a meaningful type"
    );

    // A public address: the module refuses documentation ranges.
    let result = run_ip(
        ip_golden("ifconfig-json.json"),
        Duration::from_secs(5),
        &["1.1.1.1"],
    )
    .await
    .expect("ip section");

    let BackendExtra::Ip { addresses, .. } = result.extra else {
        panic!("the IP section must carry the IP extras");
    };
    let info = &addresses[0];
    assert_eq!(
        info.network_type, want_type,
        "network_type must come from the golden"
    );
    assert_eq!(
        info.org.as_deref(),
        Some(want_org.as_str()),
        "org must come from the golden"
    );
    assert_eq!(
        info.geo.as_deref(),
        Some(want_geo.as_str()),
        "geo must be built from city, country"
    );
}
