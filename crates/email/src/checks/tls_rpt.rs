//! TLS-RPT (SMTP TLS Reporting) check, per RFC 8460.
//!
//! TLS-RPT publishes a reporting address so mail senders can deliver
//! structured failure reports about TLS negotiation problems (e.g. MTA-STS
//! policy violations, DANE mismatches). This module resolves the TXT
//! record at `_smtp._tls.<domain>`, validates the `v=TLSRPTv1` prefix, and
//! parses the `rua=` tag (which may contain one or more `mailto:` or
//! `https:` URIs).
//!
//! The returned `CheckResult` flags missing records, malformed or empty
//! `rua=` values, and records the presence of valid reporting endpoints.
//! The boolean companion return value is used by cross-validation to
//! correlate TLS reporting coverage with MTA-STS and DANE posture.

use crate::checks::util;
use crate::dns::DnsLookup;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Check TLS-RPT at _smtp._tls.<domain>.
/// Returns (CheckResult, tls_rpt_present).
#[tracing::instrument(skip_all, fields(category = "tls_rpt", domain = %domain))]
pub async fn check_tls_rpt(domain: &str, resolver: &impl DnsLookup) -> (CheckResult, bool) {
    let name = format!("_smtp._tls.{}", domain);
    let txt_records = resolver.lookup_txt(&name).await;

    let tls_rpt_records: Vec<&str> = txt_records
        .iter()
        .filter(|t| t.starts_with("v=TLSRPTv1"))
        .map(|s| s.as_str())
        .collect();

    let mut sub_checks = Vec::new();

    if tls_rpt_records.is_empty() {
        sub_checks.push(SubCheck {
            name: "absent".to_string(),
            verdict: Verdict::Info,
            detail: "no TLS-RPT record".to_string(),
        });
        let result = CheckResult::new(
            Category::TlsRpt,
            sub_checks,
            "No TLS-RPT record".to_string(),
        );
        return (result, false);
    }

    let record = tls_rpt_records[0];
    let tags = util::parse_tags(record);

    // Syntax check: must have v=TLSRPTv1 as first tag and contain at least rua
    if !tags.contains_key("rua") && tags.len() <= 1 {
        sub_checks.push(SubCheck {
            name: "invalid_syntax".to_string(),
            verdict: Verdict::Fail,
            detail: "malformed TLS-RPT record: missing required fields".to_string(),
        });
        let result = CheckResult::new(Category::TlsRpt, sub_checks, format!("TLS-RPT: {}", record));
        return (result, true);
    }

    // Check rua
    match tags.get("rua") {
        Some(rua) => {
            let uris: Vec<&str> = rua.split(',').map(|s| s.trim()).collect();
            let valid = uris
                .iter()
                .any(|u| u.starts_with("mailto:") || u.starts_with("https://"));
            if !valid {
                sub_checks.push(SubCheck {
                    name: "invalid_rua".to_string(),
                    verdict: Verdict::Fail,
                    detail: "rua contains no valid mailto: or https:// URI".to_string(),
                });
            } else {
                sub_checks.push(SubCheck {
                    name: "valid".to_string(),
                    verdict: Verdict::Pass,
                    detail: "valid TLS-RPT record".to_string(),
                });
            }
        }
        None => {
            sub_checks.push(SubCheck {
                name: "invalid_rua".to_string(),
                verdict: Verdict::Fail,
                detail: "missing rua= in TLS-RPT record".to_string(),
            });
        }
    }

    let result = CheckResult::new(Category::TlsRpt, sub_checks, format!("TLS-RPT: {}", record));

    (result, true)
}
