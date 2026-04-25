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

use std::time::Duration;

use netray_common::enrichment::EnrichmentClient;
use wiremock::matchers::{method, path_regex};
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
    Mock::given(method("GET"))
        .and(path_regex(r"^/.+/json$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1..)
        .mount(&server)
        .await;

    let backend_url = server.uri();
    assert!(
        !backend_url.is_empty(),
        "wiremock failed to expose a base URL"
    );

    // Same construction beacon uses in AppState::new() once
    // BEACON_BACKENDS__IP_URL is set.
    let client = EnrichmentClient::new(&backend_url, Duration::from_millis(5000), "beacon", None);

    let target: std::net::IpAddr = "203.0.113.42".parse().unwrap();
    let result = client.lookup(target, None).await;

    let info = result.expect("enrichment lookup must return a payload from the mock");
    assert_eq!(info.asn, Some(64500));
    assert_eq!(info.org.as_deref(), Some("Example Corp"));
    assert_eq!(info.network_role.as_deref(), Some("Hosting"));

    // wiremock's `expect(1..).mount()` records and verifies on Drop, but we
    // assert here too so a failure surfaces a clear message.
    let received = server.received_requests().await.unwrap_or_default();
    assert!(
        !received.is_empty(),
        "wiremock recorded zero requests — enrichment client did not call the backend"
    );
}
