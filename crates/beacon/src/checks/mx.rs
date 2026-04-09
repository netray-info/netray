use std::collections::HashSet;
use std::net::IpAddr;

use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, IpEnrichment, SubCheck, Verdict};

/// Check MX records for the domain.
/// Returns (CheckResult, resolved MX IPs, MX hostnames, null_mx flag).
pub async fn check_mx(
    domain: &str,
    resolver: &DnsResolver,
    enrichment: Option<&netray_common::enrichment::EnrichmentClient>,
) -> (CheckResult, Vec<IpAddr>, Vec<String>, bool) {
    let mx_records = resolver.lookup_mx(domain).await;

    if mx_records.is_empty() {
        let result = CheckResult::new(
            Category::Mx,
            vec![SubCheck {
                name: "no_mx".to_string(),
                verdict: Verdict::Fail,
                detail: "no MX records found".to_string(),
            }],
            "No MX records found".to_string(),
        );
        return (result, Vec::new(), Vec::new(), false);
    }

    let mut sub_checks = Vec::new();
    let mut all_ips = Vec::new();
    let mut mx_hosts = Vec::new();

    // Check for Null MX (RFC 7505: MX 0 .)
    if mx_records.len() == 1 && mx_records[0].0 == 0 && mx_records[0].1.trim_end_matches('.').is_empty() {
        sub_checks.push(SubCheck {
            name: "null_mx".to_string(),
            verdict: Verdict::Info,
            detail: "Null MX (RFC 7505): domain does not accept mail".to_string(),
        });
        let result = CheckResult::new(
            Category::Mx,
            sub_checks,
            "Null MX: domain does not accept mail".to_string(),
        );
        return (result, Vec::new(), Vec::new(), true);
    }
    let null_mx = false;

    // Resolve IPs for each MX host
    let mut has_ipv6 = false;
    let mut ipv4_prefixes: HashSet<u32> = HashSet::new();
    let mut ipv6_prefixes: HashSet<u64> = HashSet::new();

    for (pref, exchange) in &mx_records {
        let host = exchange.trim_end_matches('.');
        mx_hosts.push(host.to_string());

        // Check if MX points to a CNAME
        let cnames = resolver.lookup_cname(host).await;
        if !cnames.is_empty() {
            sub_checks.push(SubCheck {
                name: "mx_cname".to_string(),
                verdict: Verdict::Fail,
                detail: format!("MX `{}` points to a CNAME (RFC 5321 §5.1)", host),
            });
        }

        let ips = resolver.lookup_ips(host).await;
        if ips.is_empty() {
            sub_checks.push(SubCheck {
                name: "mx_no_addr".to_string(),
                verdict: Verdict::Warn,
                detail: format!("MX {} (priority {}) has no A/AAAA records", host, pref),
            });
        }

        for ip in &ips {
            match ip {
                IpAddr::V4(v4) => {
                    let octets = v4.octets();
                    let prefix = u32::from_be_bytes([octets[0], octets[1], octets[2], 0]);
                    ipv4_prefixes.insert(prefix);
                }
                IpAddr::V6(v6) => {
                    has_ipv6 = true;
                    let segments = v6.segments();
                    let prefix = ((segments[0] as u64) << 32)
                        | ((segments[1] as u64) << 16)
                        | (segments[2] as u64);
                    ipv6_prefixes.insert(prefix);
                }
            }
        }

        all_ips.extend(ips);
    }

    // Single MX check
    if mx_records.len() == 1 {
        sub_checks.push(SubCheck {
            name: "single_mx".to_string(),
            verdict: Verdict::Warn,
            detail: "only one MX record; no redundancy".to_string(),
        });
    }

    // No IPv6 check
    if !has_ipv6 && !all_ips.is_empty() {
        sub_checks.push(SubCheck {
            name: "no_ipv6".to_string(),
            verdict: Verdict::Info,
            detail: "no MX host has an AAAA record".to_string(),
        });
    }

    // Network diversity check
    let total_prefixes = ipv4_prefixes.len() + ipv6_prefixes.len();
    if total_prefixes == 1 && all_ips.len() > 1 {
        sub_checks.push(SubCheck {
            name: "low_network_diversity".to_string(),
            verdict: Verdict::Warn,
            detail: "all MX IPs in the same network block; no network diversity".to_string(),
        });
    }

    // If no sub-checks fired, add a pass
    if sub_checks.is_empty() {
        sub_checks.push(SubCheck {
            name: "mx_ok".to_string(),
            verdict: Verdict::Pass,
            detail: format!(
                "{} MX record(s), {} IP(s) resolved",
                mx_records.len(),
                all_ips.len()
            ),
        });
    }

    // Enrichment (best-effort) — data included in MX event for frontend badges
    let mut enrichment_data = Vec::new();
    if let Some(client) = enrichment {
        for ip in &all_ips {
            if let Some(info) = client.lookup(*ip, None).await {
                enrichment_data.push(IpEnrichment {
                    ip: ip.to_string(),
                    asn: info.asn,
                    org: info.org.clone(),
                    ip_type: info.ip_type.clone(),
                });
            }
        }
    }

    let detail = format!(
        "{} MX record(s), {} IP(s)",
        mx_records.len(),
        all_ips.len()
    );
    let mut result = CheckResult::new(Category::Mx, sub_checks, detail);
    result.enrichment = enrichment_data;

    (result, all_ips, mx_hosts, null_mx)
}
