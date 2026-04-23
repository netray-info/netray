//! DANE (DNS-Based Authentication of Named Entities) check for SMTP, per
//! RFC 7672.
//!
//! DANE binds a server's TLS certificate to the DNS via `TLSA` records
//! published under `_25._tcp.<mx-host>`. For each MX host discovered by the
//! MX check, this module performs a TLSA lookup in parallel (`join_all`)
//! and records whether at least one usable TLSA record is present. The
//! verdict does not attempt full TLS handshake validation (beacon is
//! DNS-only); presence of a syntactically valid TLSA record with a
//! supported `usage`/`selector`/`matching_type` tuple is reported as
//! `Pass`, and missing records are reported as `Info` when the domain has
//! no mail (empty MX set) or `Warn` otherwise.
//!
//! Effective DANE protection additionally requires DNSSEC on the parent
//! zone; the [`super::dnssec`] check and cross-validation surface that
//! requirement.

use futures::future::join_all;

use crate::dns::DnsLookup;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Check DANE/TLSA records for each MX hostname.
/// Returns (CheckResult, dane_has_tlsa).
#[tracing::instrument(skip_all, fields(category = "dane"))]
pub async fn check_dane(mx_hosts: &[String], resolver: &impl DnsLookup) -> (CheckResult, bool) {
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
        let host_had_error = sub_checks.len();

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

        if sub_checks.len() == host_had_error {
            sub_checks.push(SubCheck {
                name: "valid".to_string(),
                verdict: Verdict::Pass,
                detail: format!("{}: TLSA records present and valid", host),
            });
        }
    }

    if !has_tlsa {
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
