//! IP enrichment for the netray.info inspectors: ASN, cloud provider and threat-flag metadata
//! answered in-process by the registered ip module (an [`EnrichmentSource`]).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};

/// Cloud provider metadata from the enrichment API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct CloudInfo {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
}

/// Metadata about a single IP address from the enrichment API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct IpInfo {
    #[serde(default)]
    pub asn: Option<u32>,
    #[serde(default)]
    pub org: Option<String>,
    /// IP classification: "cloud", "datacenter", "residential", "vpn", etc.
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
    /// ASN network role from ipverse/as-metadata (e.g. "Midsize Transit", "Access Provider").
    #[serde(default)]
    pub network_role: Option<String>,
}

/// Returns `true` for any IP address that should not be sent to the enrichment
/// API — private, reserved, or special-purpose ranges including CGNAT
/// (100.64.0.0/10).
///
/// Uses the unified [`crate::target_policy`] blocklist.
pub fn is_private_ip(ip: IpAddr) -> bool {
    !crate::target_policy::is_allowed_target(ip)
}

/// A lookup answered inside the process, in the shape the enrichment API serves.
pub trait EnrichmentSource: Send + Sync {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Option<IpInfo>>;
}

/// Enrichment lookups for the inspectors, answered by an [`EnrichmentSource`] in this process.
pub struct EnrichmentClient {
    source: Arc<dyn EnrichmentSource>,
}

impl EnrichmentClient {
    /// A client that answers from `source` with no HTTP request; the private-address guard
    /// still runs before the source.
    pub fn in_process(source: Arc<dyn EnrichmentSource>) -> Self {
        Self { source }
    }

    /// Look up metadata for a single IP.
    ///
    /// Returns `None` for private/blocked IPs (the source is not asked) and when the source has
    /// no answer. `request_id` is accepted for the callers' tracing and not used.
    pub async fn lookup(&self, ip: IpAddr, _request_id: Option<&str>) -> Option<IpInfo> {
        if is_private_ip(ip) {
            return None;
        }
        self.source.lookup(ip).await
    }

    /// Look up metadata for multiple IPs concurrently.
    ///
    /// Deduplicates the input and silently skips private/blocked IPs.
    /// Returns only the IPs for which enrichment succeeded.
    pub async fn lookup_batch(
        &self,
        ips: &[IpAddr],
        request_id: Option<&str>,
    ) -> HashMap<IpAddr, IpInfo> {
        let rid = request_id.map(|s| s.to_owned());
        let mut seen = std::collections::HashSet::new();
        let futs: FuturesUnordered<_> = ips
            .iter()
            .copied()
            .filter(|ip| seen.insert(*ip))
            .map(|ip| {
                let rid = rid.clone();
                async move { (ip, self.lookup(ip, rid.as_deref()).await) }
            })
            .collect();

        futs.filter_map(|(ip, info)| async move { info.map(|i| (ip, i)) })
            .collect()
            .await
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
        let info: IpInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.asn, Some(16509));
        assert_eq!(info.ip_type.as_deref(), Some("cloud"));
        assert!(info.is_datacenter);
        assert!(!info.is_tor);
        let cloud = info.cloud.unwrap();
        assert_eq!(cloud.provider.as_deref(), Some("AWS"));
        assert_eq!(cloud.region.as_deref(), Some("us-east-1"));
    }

    #[test]
    fn deserializes_minimal_response() {
        let json = r#"{}"#;
        let info: IpInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.asn, None);
        assert_eq!(info.org, None);
        assert!(!info.is_tor);
    }

    #[test]
    fn lookup_skips_private_ips() {
        // is_blocked_ip covers all private ranges
        assert!(crate::ip_filter::is_blocked_ip(
            "127.0.0.1".parse().unwrap()
        ));
        assert!(crate::ip_filter::is_blocked_ip("10.0.0.1".parse().unwrap()));
        assert!(crate::ip_filter::is_blocked_ip("::1".parse().unwrap()));
        assert!(crate::ip_filter::is_blocked_ip("fc00::1".parse().unwrap()));
    }

    #[test]
    fn public_ips_not_blocked() {
        assert!(!crate::ip_filter::is_blocked_ip("8.8.8.8".parse().unwrap()));
        assert!(!crate::ip_filter::is_blocked_ip("1.1.1.1".parse().unwrap()));
        assert!(!crate::ip_filter::is_blocked_ip(
            "2606:4700::1".parse().unwrap()
        ));
    }

    #[test]
    fn is_private_ip_blocks_standard_ranges() {
        assert!(is_private_ip("127.0.0.1".parse().unwrap()));
        assert!(is_private_ip("10.0.0.1".parse().unwrap()));
        assert!(is_private_ip("192.168.1.1".parse().unwrap()));
        assert!(is_private_ip("172.16.0.1".parse().unwrap()));
        assert!(is_private_ip("::1".parse().unwrap()));
        assert!(is_private_ip("fc00::1".parse().unwrap()));
    }

    #[test]
    fn is_private_ip_blocks_cgnat() {
        assert!(is_private_ip("100.64.0.1".parse().unwrap()));
        assert!(is_private_ip("100.127.255.255".parse().unwrap()));
    }

    #[test]
    fn is_private_ip_allows_public() {
        assert!(!is_private_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_private_ip("1.1.1.1".parse().unwrap()));
        assert!(!is_private_ip("2606:4700::1".parse().unwrap()));
    }
}
