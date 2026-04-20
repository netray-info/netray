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

/// What a DNSBL query was asking about — the MX IP or the domain.
#[derive(Clone)]
enum Target {
    Ip(IpAddr),
    Domain(String),
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Ip(ip) => write!(f, "{ip}"),
            Target::Domain(d) => write!(f, "{d}"),
        }
    }
}

struct DnsblQuery {
    zone: String,
    query_name: String,
    target: Target,
}

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

    let mut queries: Vec<DnsblQuery> = Vec::new();

    for zone in &config.zones {
        if DOMAIN_ZONES.contains(&zone.as_str()) {
            queries.push(DnsblQuery {
                zone: zone.clone(),
                query_name: format!("{}.{}", domain, zone),
                target: Target::Domain(domain.to_string()),
            });
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
                queries.push(DnsblQuery {
                    zone: zone.clone(),
                    query_name: format!("{}.{}", reversed, zone),
                    target: Target::Ip(*ip),
                });
            }
        }
    }

    let futures = queries.iter().map(|q| async move {
        let lookup = tokio::time::timeout(timeout, resolver.lookup_a(&q.query_name)).await;
        (q, lookup)
    });

    let results = join_all(futures).await;

    let mut sub_checks = Vec::new();
    let mut timed_out_zones: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (query, lookup) in results {
        let values = match lookup {
            Ok(v) => v,
            Err(_) => {
                tracing::debug!(zone = %query.zone, query_name = %query.query_name, "DNSBL query timed out");
                timed_out_zones.insert(query.zone.clone());
                continue;
            }
        };

        if values.is_empty() {
            continue;
        }

        let (policy, listing): (Vec<_>, Vec<_>) = values.into_iter().partition(is_policy_response);
        let zone_slug = query.zone.replace('.', "_");

        if !listing.is_empty() {
            let codes = format_codes(&listing);
            sub_checks.push(SubCheck {
                name: format!("listed_{}", zone_slug),
                verdict: Verdict::Fail,
                detail: format!("{} listed in {} ({})", query.target, query.zone, codes),
            });
        } else if !policy.is_empty() {
            let codes = format_codes(&policy);
            sub_checks.push(SubCheck {
                name: format!("policy_response_{}", zone_slug),
                verdict: Verdict::Info,
                detail: format!(
                    "{} rejected the query ({}); results from this zone are unreliable",
                    query.zone, codes
                ),
            });
        }
    }

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

/// DNSBL error/policy responses live in `127.255.255.0/24`. Every major DNSBL
/// follows this convention; Spamhaus documents `127.255.255.252`–`.255` for
/// typo, anonymous query, blocked public resolver, and rate limit respectively.
fn is_policy_response(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 127 && o[1] == 255 && o[2] == 255
}

fn format_codes(ips: &[Ipv4Addr]) -> String {
    let mut codes: Vec<String> = ips.iter().map(|ip| ip.to_string()).collect();
    codes.sort();
    codes.dedup();
    codes.join(", ")
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

    #[test]
    fn test_policy_response_range() {
        // Spamhaus "query via public resolver" sentinel
        assert!(is_policy_response(&Ipv4Addr::new(127, 255, 255, 254)));
        assert!(is_policy_response(&Ipv4Addr::new(127, 255, 255, 252)));
        assert!(is_policy_response(&Ipv4Addr::new(127, 255, 255, 0)));
        // Real listing codes are NOT policy responses
        assert!(!is_policy_response(&Ipv4Addr::new(127, 0, 0, 2))); // SBL
        assert!(!is_policy_response(&Ipv4Addr::new(127, 0, 0, 10))); // PBL
        assert!(!is_policy_response(&Ipv4Addr::new(127, 0, 0, 4))); // XBL
    }
}
