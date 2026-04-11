use futures::future::join_all;

use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Check DANE/TLSA records for each MX hostname.
/// Returns (CheckResult, dane_has_tlsa).
pub async fn check_dane(mx_hosts: &[String], resolver: &DnsResolver) -> (CheckResult, bool) {
    if mx_hosts.is_empty() {
        let sub_checks = vec![SubCheck {
            name: "absent".to_string(),
            verdict: Verdict::Info,
            detail: "no MX hosts to check for DANE".to_string(),
        }];
        let result = CheckResult::new(Category::Dane, sub_checks, "No MX hosts".to_string());
        return (result, false);
    }

    // Parallelize TLSA lookups for all MX hosts
    let tlsa_futures = mx_hosts.iter().map(|host| {
        let tlsa_name = format!("_25._tcp.{}", host);
        async move {
            let records = resolver.lookup_tlsa(&tlsa_name).await;
            (host, records)
        }
    });
    let tlsa_results = join_all(tlsa_futures).await;

    let mut sub_checks = Vec::new();
    let mut has_tlsa = false;

    for (host, records) in tlsa_results {
        if records.is_empty() {
            continue;
        }

        has_tlsa = true;

        for record in &records {
            // Validate usage (0-3)
            if record.usage > 3 {
                sub_checks.push(SubCheck {
                    name: "invalid_usage".to_string(),
                    verdict: Verdict::Fail,
                    detail: format!("{}: TLSA usage {} out of range 0-3", host, record.usage),
                });
            }

            // Validate selector (0-1)
            if record.selector > 1 {
                sub_checks.push(SubCheck {
                    name: "invalid_selector".to_string(),
                    verdict: Verdict::Fail,
                    detail: format!(
                        "{}: TLSA selector {} out of range 0-1",
                        host, record.selector
                    ),
                });
            }

            // Validate matching type (0-2)
            if record.matching_type > 2 {
                sub_checks.push(SubCheck {
                    name: "invalid_matching".to_string(),
                    verdict: Verdict::Fail,
                    detail: format!(
                        "{}: TLSA matching type {} out of range 0-2",
                        host, record.matching_type
                    ),
                });
            }
        }
    }

    if has_tlsa && sub_checks.is_empty() {
        sub_checks.push(SubCheck {
            name: "valid".to_string(),
            verdict: Verdict::Pass,
            detail: "DANE TLSA records present and valid".to_string(),
        });
    } else if !has_tlsa {
        sub_checks.push(SubCheck {
            name: "absent".to_string(),
            verdict: Verdict::Info,
            detail: "no TLSA records on any MX host".to_string(),
        });
    }

    let detail = if has_tlsa {
        "DANE TLSA records found".to_string()
    } else {
        "No DANE TLSA records".to_string()
    };

    let result = CheckResult::new(Category::Dane, sub_checks, detail);
    (result, has_tlsa)
}
