//! Integration test for `BEACON_BACKENDS__IP_URL` (project-review SDD Req 12).
//!
//! Spins up a `wiremock::MockServer`, points the beacon enrichment client at
//! it, and verifies that the client actually issues a request when given an
//! IP. We test the enrichment client wiring directly rather than running the
//! full inspect handler — the inspect path requires real DNS resolution to
//! find MX IPs, which we cannot stub at the resolver layer without intrusive
//! changes. The test that matters here is "does configuring the IP backend
//! cause an outbound enrichment HTTP request?" — and that we can answer
//! cleanly with the MockServer alone.
//!
//! Note: `EnrichmentClient::lookup` short-circuits on private/reserved IPs
//! via `is_allowed_target`, so the test must use a publicly-routable address
//! (RFC 5737 doc-prefix and RFC 1918 ranges all return None before the
//! backend is hit).

use std::time::Duration;

use netray_common::enrichment::EnrichmentClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn enrichment_client_calls_backend_when_url_set() {
    // Synthetic enrichment payload — fields match netray_common::enrichment::IpInfo.
    let body = serde_json::json!({
        "asn": 64500,
        "org": "Example Corp",
        "type": "datacenter",
        "is_tor": false,
        "is_vpn": false,
        "is_datacenter": true,
        "is_spamhaus": false,
        "is_c2": false,
        "network_role": "Hosting",
    });

    let server = MockServer::start().await;
    // `EnrichmentClient::lookup` calls `GET /network/json?ip=<IP>`.
    // wiremock `path()` matches the path component without the query string.
    Mock::given(method("GET"))
        .and(path("/network/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1..)
        .mount(&server)
        .await;

    let backend_url = server.uri();

    // Same construction beacon uses in AppState::new() once
    // BEACON_BACKENDS__IP_URL is set.
    let client = EnrichmentClient::new(&backend_url, Duration::from_millis(5000), "beacon", None);

    // Use a public IP. Reserved ranges (RFC 1918, RFC 5737 doc-prefix, etc.)
    // are short-circuited by `is_allowed_target` before the backend is hit.
    let target: std::net::IpAddr = "8.8.8.8".parse().unwrap();
    let result = client.lookup(target, None).await;

    let info = result.expect("enrichment lookup must return a payload from the mock");
    assert_eq!(info.asn, Some(64500));
    assert_eq!(info.org.as_deref(), Some("Example Corp"));
    assert_eq!(info.network_role.as_deref(), Some("Hosting"));

    // Belt-and-braces: wiremock's `expect(1..).mount()` already verifies on
    // Drop, but assert here so a failure surfaces a specific message.
    let received = server.received_requests().await.unwrap_or_default();
    assert!(
        !received.is_empty(),
        "wiremock recorded zero requests — enrichment client did not call the backend"
    );
}
