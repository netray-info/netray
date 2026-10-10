// Contract tests: verify tlsight can deserialize ifconfig-rs API responses.
//
// ifconfig-rs serializes its `Network` struct at GET /network/json?ip=<addr>.
// tlsight calls this endpoint (via EnrichmentClient) and deserializes the body into
// `netray_common::enrichment::IpInfo`.
//
// If ifconfig-rs renames a field (e.g. "type" → "network_type", "org" → "asn_org"),
// `#[serde(default)]` silently produces None rather than an error, so IP badges
// in the TLS inspection response render empty. These tests catch that before deployment.
//
// Run with: cargo test --test contract

use netray_common::enrichment::{CloudInfo, IpInfo};

/// The fixture is ifconfig-rs's `Network` struct serialized to JSON,
/// exactly as returned by GET /network/json?ip=8.8.8.8 on a populated instance.
const NETWORK_FIXTURE: &str = include_str!("fixtures/network_response.json");

#[test]
fn deserializes_full_network_response() {
    let info: IpInfo = serde_json::from_str(NETWORK_FIXTURE)
        .expect("fixture must parse into IpInfo without error");

    assert_eq!(info.asn, Some(15169), "asn field missing or wrong");
    assert_eq!(
        info.org.as_deref(),
        Some("Google LLC"),
        "org field missing or wrong"
    );
    assert_eq!(
        info.ip_type.as_deref(),
        Some("cloud"),
        "type field missing (check serde rename = \"type\" on ip_type)"
    );
    assert!(info.is_datacenter, "is_datacenter should be true");
    assert!(!info.is_tor, "is_tor should be false");
    assert!(!info.is_vpn, "is_vpn should be false");
    assert!(!info.is_spamhaus, "is_spamhaus should be false");
    assert!(!info.is_c2, "is_c2 should be false");
}

#[test]
fn deserializes_cloud_sub_object() {
    let info: IpInfo = serde_json::from_str(NETWORK_FIXTURE).expect("fixture must parse");

    let cloud: CloudInfo = info.cloud.expect("cloud field missing");
    assert_eq!(
        cloud.provider.as_deref(),
        Some("gcp"),
        "cloud.provider missing"
    );
    assert_eq!(
        cloud.service.as_deref(),
        Some("DNS"),
        "cloud.service missing"
    );
    assert_eq!(
        cloud.region.as_deref(),
        Some("us-central1"),
        "cloud.region missing"
    );
}

#[test]
fn deserializes_minimal_network_response() {
    // ifconfig-rs returns null for optional fields (e.g. residential IP with no cloud match).
    let minimal = r#"{
        "type": "residential",
        "infra_type": "residential",
        "is_internal": false,
        "is_datacenter": false,
        "is_vpn": false,
        "is_tor": false,
        "is_bot": false,
        "is_c2": false,
        "is_spamhaus": false,
        "cloud": null,
        "vpn": null,
        "bot": null
    }"#;

    let info: IpInfo = serde_json::from_str(minimal).expect("minimal fixture must parse");

    assert_eq!(info.ip_type.as_deref(), Some("residential"));
    assert!(
        info.asn.is_none(),
        "asn should be None for minimal response"
    );
    assert!(
        info.org.is_none(),
        "org should be None for minimal response"
    );
    assert!(
        info.cloud.is_none(),
        "cloud should be None for minimal response"
    );
    assert!(!info.is_datacenter);
    assert!(!info.is_tor);
}

#[test]
fn deserializes_tor_exit_node_response() {
    let tor = r#"{
        "asn": 4224,
        "org": "Tor Project",
        "type": "tor",
        "infra_type": "residential",
        "is_internal": false,
        "is_datacenter": false,
        "is_vpn": false,
        "is_tor": true,
        "is_bot": false,
        "is_c2": false,
        "is_spamhaus": false,
        "cloud": null,
        "vpn": null,
        "bot": null
    }"#;

    let info: IpInfo = serde_json::from_str(tor).expect("tor fixture must parse");
    assert!(info.is_tor, "is_tor should be true");
    assert_eq!(info.ip_type.as_deref(), Some("tor"));
}
