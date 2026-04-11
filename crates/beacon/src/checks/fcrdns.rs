use std::net::IpAddr;

use futures::future::join_all;

use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Check Forward-Confirmed reverse DNS for each MX IP.
pub async fn check_fcrdns(mx_ips: &[IpAddr], resolver: &DnsResolver) -> CheckResult {
    if mx_ips.is_empty() {
        let sub_checks = vec![SubCheck {
            name: "no_ips".to_string(),
            verdict: Verdict::Info,
            detail: "no MX IPs to check".to_string(),
        }];
        return CheckResult::new(Category::Fcrdns, sub_checks, "No MX IPs".to_string());
    }

    // Parallelize PTR lookups for all IPs
    let ptr_futures = mx_ips.iter().map(|ip| async move {
        let ptr_names = resolver.lookup_ptr(*ip).await;
        (*ip, ptr_names)
    });
    let ptr_results: Vec<(IpAddr, Vec<String>)> = join_all(ptr_futures).await;

    // Collect all forward lookup requirements: (ip, ptr_name)
    let forward_futures: Vec<_> = ptr_results
        .iter()
        .flat_map(|(ip, ptr_names)| {
            ptr_names.iter().map(move |ptr_name| {
                let ptr_name = ptr_name.trim_end_matches('.').to_string();
                let ip = *ip;
                async move {
                    let forward_ips = resolver.lookup_ips(&ptr_name).await;
                    (ip, forward_ips)
                }
            })
        })
        .collect();

    // Group forward results by IP
    let forward_results: Vec<(IpAddr, Vec<IpAddr>)> = join_all(forward_futures).await;

    let mut sub_checks = Vec::new();

    for (ip, ptr_names) in &ptr_results {
        if ptr_names.is_empty() {
            sub_checks.push(SubCheck {
                name: "fcrdns_fail".to_string(),
                verdict: Verdict::Warn,
                detail: format!(
                    "FCrDNS failed for {}: no PTR record (required by Google/Yahoo since Feb 2024)",
                    ip
                ),
            });
            continue;
        }

        // Check if any forward lookup confirmed this IP
        let confirmed = forward_results
            .iter()
            .any(|(fwd_ip, fwd_ips)| fwd_ip == ip && fwd_ips.contains(ip));

        if confirmed {
            sub_checks.push(SubCheck {
                name: "fcrdns_pass".to_string(),
                verdict: Verdict::Pass,
                detail: format!("FCrDNS confirmed for {}", ip),
            });
        } else {
            sub_checks.push(SubCheck {
                name: "fcrdns_fail".to_string(),
                verdict: Verdict::Warn,
                detail: format!(
                    "FCrDNS failed for {}: PTR does not resolve back (required by Google/Yahoo since Feb 2024)",
                    ip
                ),
            });
        }
    }

    let detail = format!("FCrDNS checked for {} IP(s)", mx_ips.len());
    CheckResult::new(Category::Fcrdns, sub_checks, detail)
}
