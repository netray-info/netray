//! IP enrichment via ifconfig-rs compatible API.
//!
//! Provides ASN, organization, IP type, and threat flag metadata for IP addresses
//! found in DNS results. All errors are gracefully handled — enrichment never
//! blocks or fails the main DNS query flow.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};

/// Cloud provider metadata from the ifconfig API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInfo {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
}

/// Metadata about a single IP address from the ifconfig API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpInfo {
    #[serde(default)]
    pub asn: Option<u32>,
    #[serde(default)]
    pub org: Option<String>,
    /// IP type: "datacenter", "cloud", "residential", "mobile", etc.
    #[serde(default, rename = "type")]
    pub ip_type: Option<String>,
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

/// Service for looking up IP metadata via an ifconfig-rs compatible API.
pub struct IpEnrichmentService {
    client: reqwest::Client,
    base_url: String,
    cache: moka::future::Cache<IpAddr, Option<IpInfo>>,
}

impl IpEnrichmentService {
    /// Create a new enrichment service.
    ///
    /// - `base_url`: ifconfig API base (e.g. `https://ip.netray.info`)
    /// - `timeout`: HTTP request timeout
    pub fn new(base_url: &str, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent("prism")
            .build()
            .expect("failed to build reqwest client");

        let cache = moka::future::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(300))
            .build();

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
            cache,
        }
    }

    /// Look up metadata for a single IP. Returns `None` on any error or for private IPs.
    pub async fn lookup(&self, ip: IpAddr) -> Option<IpInfo> {
        if is_private_ip(ip) {
            return None;
        }

        if let Some(cached) = self.cache.get(&ip).await {
            metrics::counter!("prism_enrichment_cache_hits_total").increment(1);
            return cached;
        }

        metrics::counter!("prism_enrichment_requests_total").increment(1);

        let url = format!("{}/network/json?ip={}", self.base_url, ip);
        let result = match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => resp.json::<IpInfo>().await.ok(),
            Ok(resp) => {
                tracing::debug!(ip = %ip, status = %resp.status(), "enrichment lookup failed");
                None
            }
            Err(e) => {
                tracing::debug!(ip = %ip, error = %e, "enrichment lookup error");
                None
            }
        };

        self.cache.insert(ip, result.clone()).await;
        result
    }

    /// Look up metadata for multiple IPs in parallel, returning only successful results.
    pub async fn lookup_batch(&self, ips: &[IpAddr]) -> HashMap<IpAddr, IpInfo> {
        // Deduplicate IPs.
        let mut unique: Vec<IpAddr> = Vec::with_capacity(ips.len());
        for ip in ips {
            if !unique.contains(ip) {
                unique.push(*ip);
            }
        }

        let futs: FuturesUnordered<_> = unique
            .into_iter()
            .map(|ip| {
                let this = &self;
                async move { (ip, this.lookup(ip).await) }
            })
            .collect();

        futs.filter_map(|(ip, info)| async move { info.map(|i| (ip, i)) })
            .collect()
            .await
    }
}

/// Check if an IP is private/reserved (should not be sent to external enrichment API).
fn is_private_ip(ip: IpAddr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || match ip {
            IpAddr::V4(v4) => {
                let o = v4.octets();
                // RFC 1918
                o[0] == 10
                    || (o[0] == 172 && (o[1] & 0xF0) == 16)
                    || (o[0] == 192 && o[1] == 168)
                    // Link-local
                    || (o[0] == 169 && o[1] == 254)
                    // CGNAT
                    || (o[0] == 100 && (o[1] & 0xC0) == 64)
            }
            IpAddr::V6(v6) => {
                let seg = v6.segments();
                // Link-local fe80::/10
                (seg[0] & 0xFFC0) == 0xFE80
                // ULA fc00::/7
                || (seg[0] & 0xFE00) == 0xFC00
            }
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_ips_detected() {
        assert!(is_private_ip("127.0.0.1".parse().unwrap()));
        assert!(is_private_ip("10.0.0.1".parse().unwrap()));
        assert!(is_private_ip("192.168.1.1".parse().unwrap()));
        assert!(is_private_ip("172.16.0.1".parse().unwrap()));
        assert!(is_private_ip("169.254.1.1".parse().unwrap()));
        assert!(is_private_ip("100.64.0.1".parse().unwrap()));
        assert!(is_private_ip("fe80::1".parse().unwrap()));
        assert!(is_private_ip("fc00::1".parse().unwrap()));
    }

    #[test]
    fn public_ips_allowed() {
        assert!(!is_private_ip("1.1.1.1".parse().unwrap()));
        assert!(!is_private_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_private_ip("2606:4700::1".parse().unwrap()));
    }
}
