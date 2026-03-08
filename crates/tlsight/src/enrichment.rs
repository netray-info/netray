//! IP enrichment via ifconfig-rs backend-to-backend calls.
//!
//! When configured, fetches ASN, cloud provider, network type, and threat
//! indicators for each inspected IP. Enrichment runs concurrently with TLS
//! handshakes and DNS lookups. Failures are non-fatal — they never block or
//! fail the inspection.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub struct EnrichmentClient {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IpEnrichment {
    #[serde(default, rename = "type")]
    pub network_type: Option<String>,
    #[serde(default)]
    pub asn: Option<u32>,
    #[serde(default)]
    pub org: Option<String>,
    #[serde(default)]
    pub cloud: Option<CloudInfo>,
    #[serde(default)]
    pub is_tor: bool,
    #[serde(default)]
    pub is_vpn: bool,
    #[serde(default)]
    pub is_datacenter: bool,
    #[serde(default)]
    pub is_spamhaus: bool,
    #[serde(default)]
    pub is_c2: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CloudInfo {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
}

impl EnrichmentClient {
    pub fn new(base_url: &str, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("tlsight")
            .timeout(timeout)
            .build()
            .expect("failed to build enrichment HTTP client");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
        }
    }

    /// Look up enrichment data for a single IP. Returns `None` on any error.
    pub async fn lookup(&self, ip: IpAddr) -> Option<IpEnrichment> {
        let url = format!(
            "{}/network?ip={}&fields=asn,org,type,cloud,is_tor,is_vpn,is_datacenter,is_spamhaus,is_c2",
            self.base_url, ip
        );
        match self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => match resp.json::<IpEnrichment>().await {
                Ok(info) => Some(info),
                Err(e) => {
                    tracing::debug!(ip = %ip, error = %e, "enrichment parse failed");
                    None
                }
            },
            Ok(resp) => {
                tracing::debug!(ip = %ip, status = %resp.status(), "enrichment HTTP error");
                None
            }
            Err(e) => {
                tracing::debug!(ip = %ip, error = %e, "enrichment request failed");
                None
            }
        }
    }

    /// Look up enrichment data for multiple IPs concurrently.
    /// Skips private/loopback IPs and deduplicates.
    pub async fn lookup_batch(&self, ips: &[IpAddr]) -> HashMap<IpAddr, IpEnrichment> {
        let mut seen = std::collections::HashSet::new();
        let mut futures = FuturesUnordered::new();

        for &ip in ips {
            if !seen.insert(ip) {
                continue;
            }
            if is_private_or_loopback(ip) {
                continue;
            }
            futures.push(async move {
                let result = self.lookup(ip).await;
                (ip, result)
            });
        }

        let mut results = HashMap::new();
        while let Some((ip, enrichment)) = futures.next().await {
            if let Some(info) = enrichment {
                results.insert(ip, info);
            }
        }
        results
    }
}

fn is_private_or_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_full_response() {
        let json = r#"{
            "type": "cloud",
            "asn": 16509,
            "org": "Amazon.com, Inc.",
            "cloud": { "provider": "AWS", "region": "us-east-1", "service": "EC2" },
            "is_tor": false, "is_vpn": false, "is_datacenter": true,
            "is_spamhaus": false, "is_c2": false
        }"#;
        let info: IpEnrichment = serde_json::from_str(json).unwrap();
        assert_eq!(info.asn, Some(16509));
        assert_eq!(info.network_type.as_deref(), Some("cloud"));
        assert!(info.is_datacenter);
        assert!(!info.is_tor);
        let cloud = info.cloud.unwrap();
        assert_eq!(cloud.provider.as_deref(), Some("AWS"));
        assert_eq!(cloud.region.as_deref(), Some("us-east-1"));
    }

    #[test]
    fn deserializes_minimal_response() {
        let json = r#"{}"#;
        let info: IpEnrichment = serde_json::from_str(json).unwrap();
        assert_eq!(info.asn, None);
        assert_eq!(info.org, None);
        assert!(!info.is_tor);
    }

    #[test]
    fn private_ips_detected() {
        assert!(is_private_or_loopback("127.0.0.1".parse().unwrap()));
        assert!(is_private_or_loopback("10.0.0.1".parse().unwrap()));
        assert!(is_private_or_loopback("192.168.1.1".parse().unwrap()));
        assert!(is_private_or_loopback("::1".parse().unwrap()));
        assert!(is_private_or_loopback("fc00::1".parse().unwrap()));
        assert!(is_private_or_loopback("fe80::1".parse().unwrap()));
    }

    #[test]
    fn public_ips_not_private() {
        assert!(!is_private_or_loopback("8.8.8.8".parse().unwrap()));
        assert!(!is_private_or_loopback("1.1.1.1".parse().unwrap()));
        assert!(!is_private_or_loopback("2606:4700::1".parse().unwrap()));
    }
}
