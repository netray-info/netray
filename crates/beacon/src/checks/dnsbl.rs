use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use futures::future::join_all;

use crate::config::DnsblConfig;
use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// IPv4 zones that don't support IPv6 lookups.
const IPV4_ONLY_ZONES: &[&str] = &[
    "zen.spamhaus.org",
    "b.barracudacentral.org",
    "bl.spamcop.net",
];

/// Domain-based zones (query target domain, not IP).
const DOMAIN_ZONES: &[&str] = &["dbl.spamhaus.org"];

/// Check DNSBLs for MX IPs.
pub async fn check_dnsbl(
    mx_ips: &[IpAddr],
    domain: &str,
    config: &DnsblConfig,
    resolver: &DnsResolver,
) -> CheckResult {
    let timeout = Duration::from_millis(config.timeout_ms);

    if mx_ips.is_empty() {
        let sub_checks = vec![SubCheck {
            name: "no_ips".to_string(),
            verdict: Verdict::Info,
            detail: "no MX IPs to check".to_string(),
        }];
        return CheckResult::new(Category::Dnsbl, sub_checks, "No MX IPs".to_string());
    }

    // Build all (zone, query_name) pairs to look up in parallel
    let mut queries: Vec<(String, String)> = Vec::new();

    for zone in &config.zones {
        if DOMAIN_ZONES.contains(&zone.as_str()) {
            let query_name = format!("{}.{}", domain, zone);
            queries.push((zone.clone(), query_name));
        } else {
            for ip in mx_ips {
                let is_ipv4_only = IPV4_ONLY_ZONES.contains(&zone.as_str());
                if is_ipv4_only && ip.is_ipv6() {
                    continue;
                }
                let reversed = match ip {
                    IpAddr::V4(v4) => reverse_ipv4(*v4),
                    IpAddr::V6(v6) => reverse_ipv6(*v6),
                };
                let query_name = format!("{}.{}", reversed, zone);
                queries.push((zone.clone(), query_name));
            }
        }
    }

    // Execute all queries in parallel
    let futures = queries.iter().map(|(zone, query_name)| {
        let zone = zone.clone();
        let query_name = query_name.clone();
        async move {
            match tokio::time::timeout(timeout, resolver.lookup_exists(&query_name)).await {
                Ok(true) => Some((zone, query_name, true)),
                Ok(false) => None,
                Err(_) => {
                    tracing::debug!(zone = %zone, query_name = %query_name, "DNSBL query timed out");
                    Some((zone, query_name, false)) // timed out, treat as timeout marker
                }
            }
        }
    });

    let results: Vec<_> = join_all(futures).await;

    let mut sub_checks = Vec::new();

    // Track which zones timed out vs listed
    let mut timed_out_zones: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (zone, query_name, listed) in results.into_iter().flatten() {
        if listed {
            // Ok(true) -> Some((zone, _, true)) means actually listed
            let zone_slug = zone.replace('.', "_");
            // Determine if it's a domain-based or IP-based listing
            if DOMAIN_ZONES.contains(&zone.as_str()) {
                sub_checks.push(SubCheck {
                    name: format!("listed_{}", zone_slug),
                    verdict: Verdict::Fail,
                    detail: format!("domain {} listed in {}", domain, zone),
                });
            } else {
                // Extract original IP from query name
                sub_checks.push(SubCheck {
                    name: format!("listed_{}", zone_slug),
                    verdict: Verdict::Fail,
                    detail: format!(
                        "{} listed in {}",
                        query_name.trim_end_matches(&format!(".{}", zone)),
                        zone
                    ),
                });
            }
        } else {
            // Timed out
            timed_out_zones.insert(zone);
        }
    }

    // Add one info sub-check per timed out zone
    for zone in timed_out_zones {
        sub_checks.push(SubCheck {
            name: "zone_unreachable".to_string(),
            verdict: Verdict::Info,
            detail: format!("zone {} unreachable", zone),
        });
    }

    if sub_checks.is_empty() {
        sub_checks.push(SubCheck {
            name: "clean".to_string(),
            verdict: Verdict::Pass,
            detail: "not listed in any DNSBL".to_string(),
        });
    }

    let detail = format!(
        "checked {} zone(s) for {} IP(s)",
        config.zones.len(),
        mx_ips.len()
    );
    CheckResult::new(Category::Dnsbl, sub_checks, detail)
}

/// Reverse an IPv4 address: 1.2.3.4 -> "4.3.2.1"
pub fn reverse_ipv4(ip: Ipv4Addr) -> String {
    let o = ip.octets();
    format!("{}.{}.{}.{}", o[3], o[2], o[1], o[0])
}

/// Reverse an IPv6 address to nibble form:
/// 2001:db8::1 -> "1.0.0.0...8.b.d.0.1.0.0.2"
pub fn reverse_ipv6(ip: Ipv6Addr) -> String {
    let octets = ip.octets();
    let mut nibbles = String::with_capacity(63);
    for byte in octets.iter().rev() {
        if !nibbles.is_empty() {
            nibbles.push('.');
        }
        nibbles.push_str(&format!("{:x}.{:x}", byte & 0x0f, (byte >> 4) & 0x0f));
    }
    nibbles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverse_ipv4() {
        assert_eq!(reverse_ipv4(Ipv4Addr::new(1, 2, 3, 4)), "4.3.2.1");
    }

    #[test]
    fn test_reverse_ipv6() {
        let ip: Ipv6Addr = "2001:0db8:0000:0000:0000:0000:0000:0001".parse().unwrap();
        assert_eq!(
            reverse_ipv6(ip),
            "1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.8.b.d.0.1.0.0.2"
        );
    }

    #[test]
    fn test_reverse_ipv6_shorthand() {
        let ip: Ipv6Addr = "2001:db8::1".parse().unwrap();
        assert_eq!(
            reverse_ipv6(ip),
            "1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.8.b.d.0.1.0.0.2"
        );
    }
}
