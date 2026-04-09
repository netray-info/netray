use std::net::IpAddr;

use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Check Forward-Confirmed reverse DNS for each MX IP.
pub async fn check_fcrdns(
    mx_ips: &[IpAddr],
    resolver: &DnsResolver,
) -> CheckResult {
    let mut sub_checks = Vec::new();

    if mx_ips.is_empty() {
        sub_checks.push(SubCheck {
            name: "no_ips".to_string(),
            verdict: Verdict::Info,
            detail: "no MX IPs to check".to_string(),
        });
        return CheckResult::new(Category::Fcrdns, sub_checks, "No MX IPs".to_string());
    }

    for ip in mx_ips {
        let ptr_names = resolver.lookup_ptr(*ip).await;

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

        let mut confirmed = false;
        for ptr_name in &ptr_names {
            let forward_ips = resolver.lookup_ips(ptr_name.trim_end_matches('.')).await;
            if forward_ips.contains(ip) {
                confirmed = true;
                break;
            }
        }

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
