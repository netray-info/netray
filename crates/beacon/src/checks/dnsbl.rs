use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

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
    let mut sub_checks = Vec::new();
    let timeout = Duration::from_millis(config.timeout_ms);

    if mx_ips.is_empty() {
        sub_checks.push(SubCheck {
            name: "no_ips".to_string(),
            verdict: Verdict::Info,
            detail: "no MX IPs to check".to_string(),
        });
        return CheckResult::new(Category::Dnsbl, sub_checks, "No MX IPs".to_string());
    }

    for zone in &config.zones {
        if DOMAIN_ZONES.contains(&zone.as_str()) {
            // Domain-based lookup
            let query_name = format!("{}.{}", domain, zone);
            match tokio::time::timeout(timeout, resolver.lookup_exists(&query_name)).await {
                Ok(true) => {
                    let zone_slug = zone.replace('.', "_");
                    sub_checks.push(SubCheck {
                        name: format!("listed_{}", zone_slug),
                        verdict: Verdict::Fail,
                        detail: format!("domain {} listed in {}", domain, zone),
                    });
                }
                Ok(false) => {} // Not listed
                Err(_) => {
                    tracing::debug!(zone = %zone, "DNSBL zone timed out");
                    sub_checks.push(SubCheck {
                        name: "zone_unreachable".to_string(),
                        verdict: Verdict::Info,
                        detail: format!("zone {} unreachable", zone),
                    });
                }
            }
            continue;
        }

        // IP-based lookup
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

            match tokio::time::timeout(timeout, resolver.lookup_exists(&query_name)).await {
                Ok(true) => {
                    let zone_slug = zone.replace('.', "_");
                    sub_checks.push(SubCheck {
                        name: format!("listed_{}", zone_slug),
                        verdict: Verdict::Fail,
                        detail: format!("IP {} listed in {}", ip, zone),
                    });
                }
                Ok(false) => {} // Not listed
                Err(_) => {
                    tracing::debug!(zone = %zone, ip = %ip, "DNSBL query timed out");
                    sub_checks.push(SubCheck {
                        name: "zone_unreachable".to_string(),
                        verdict: Verdict::Info,
                        detail: format!("zone {} unreachable for {}", zone, ip),
                    });
                }
            }
        }
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
        assert_eq!(
            reverse_ipv4(Ipv4Addr::new(1, 2, 3, 4)),
            "4.3.2.1"
        );
    }

    #[test]
    fn test_reverse_ipv6() {
        let ip: Ipv6Addr = "2001:0db8:0000:0000:0000:0000:0000:0001"
            .parse()
            .unwrap();
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
