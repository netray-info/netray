use crate::checks::util;
use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Check DMARC for the domain.
/// Returns (CheckResult, dmarc_policy, dmarc_sp, rua_external_auth_ok).
pub async fn check_dmarc(
    domain: &str,
    resolver: &DnsResolver,
) -> (CheckResult, Option<String>, Option<String>, bool) {
    let dmarc_name = format!("_dmarc.{}", domain);
    let txt_records = resolver.lookup_txt(&dmarc_name).await;

    let dmarc_records: Vec<&str> = txt_records
        .iter()
        .filter(|t| t.starts_with("v=DMARC1"))
        .map(|s| s.as_str())
        .collect();

    let mut sub_checks = Vec::new();

    if dmarc_records.is_empty() {
        sub_checks.push(SubCheck {
            name: "no_dmarc".to_string(),
            verdict: Verdict::Fail,
            detail: "no DMARC record found".to_string(),
        });
        let result = CheckResult::new(Category::Dmarc, sub_checks, "No DMARC record".to_string());
        return (result, None, None, true);
    }

    if dmarc_records.len() > 1 {
        sub_checks.push(SubCheck {
            name: "multiple_dmarc".to_string(),
            verdict: Verdict::Fail,
            detail: format!("{} DMARC records found", dmarc_records.len()),
        });
        let result = CheckResult::new(
            Category::Dmarc,
            sub_checks,
            "Multiple DMARC records".to_string(),
        );
        return (result, None, None, true);
    }

    let record = dmarc_records[0];
    let tags = util::parse_tags(record);

    let policy = tags.get("p").cloned();
    let sub_policy = tags.get("sp").cloned();
    let pct = tags
        .get("pct")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(100);
    let rua = tags.get("rua").cloned();
    let ruf = tags.get("ruf").cloned();
    let fo = tags.get("fo").cloned();
    let ri = tags.get("ri").cloned();

    // Policy check
    match policy.as_deref() {
        Some("reject") => {
            sub_checks.push(SubCheck {
                name: "policy_reject".to_string(),
                verdict: Verdict::Pass,
                detail: "p=reject".to_string(),
            });
        }
        Some("quarantine") => {
            sub_checks.push(SubCheck {
                name: "policy_quarantine".to_string(),
                verdict: Verdict::Warn,
                detail: "partial enforcement".to_string(),
            });
        }
        Some("none") => {
            sub_checks.push(SubCheck {
                name: "policy_none".to_string(),
                verdict: Verdict::Warn,
                detail: "no enforcement".to_string(),
            });
        }
        _ => {
            sub_checks.push(SubCheck {
                name: "policy_missing".to_string(),
                verdict: Verdict::Fail,
                detail: "missing or invalid p= tag".to_string(),
            });
        }
    }

    // Percentage checks
    if pct == 0 {
        sub_checks.push(SubCheck {
            name: "pct_zero".to_string(),
            verdict: Verdict::Warn,
            detail: "effectively p=none".to_string(),
        });
    } else if pct < 100 && policy.as_deref() == Some("reject") {
        sub_checks.push(SubCheck {
            name: "pct_partial".to_string(),
            verdict: Verdict::Info,
            detail: "transitional rollout".to_string(),
        });
    }

    // Report URIs
    let mut rua_external_auth_ok = true;

    if let Some(rua_val) = &rua {
        let rua_domains = extract_report_domains(rua_val);
        for rua_domain in &rua_domains {
            if *rua_domain != domain {
                // External domain — check authorization
                let auth_name = format!(
                    "{}._report._dmarc.{}",
                    domain, rua_domain
                );
                let auth_records = resolver.lookup_txt(&auth_name).await;
                let authorized = auth_records
                    .iter()
                    .any(|t| t.starts_with("v=DMARC1"));
                if !authorized {
                    rua_external_auth_ok = false;
                    sub_checks.push(SubCheck {
                        name: "rua_auth".to_string(),
                        verdict: Verdict::Warn,
                        detail: format!(
                            "external report destination not authorized: {}",
                            rua_domain
                        ),
                    });
                }
            }
        }
    } else {
        sub_checks.push(SubCheck {
            name: "no_rua".to_string(),
            verdict: Verdict::Warn,
            detail: "no aggregate reports".to_string(),
        });
    }

    if let Some(ruf_val) = &ruf {
        let ruf_domains = extract_report_domains(ruf_val);
        for ruf_domain in &ruf_domains {
            if *ruf_domain != domain {
                let auth_name = format!(
                    "{}._report._dmarc.{}",
                    domain, ruf_domain
                );
                let auth_records = resolver.lookup_txt(&auth_name).await;
                let authorized = auth_records
                    .iter()
                    .any(|t| t.starts_with("v=DMARC1"));
                if !authorized {
                    sub_checks.push(SubCheck {
                        name: "ruf_auth".to_string(),
                        verdict: Verdict::Warn,
                        detail: format!(
                            "external forensic report destination not authorized: {}",
                            ruf_domain
                        ),
                    });
                }
            }
        }
    } else {
        sub_checks.push(SubCheck {
            name: "no_ruf".to_string(),
            verdict: Verdict::Info,
            detail: "no forensic reports configured".to_string(),
        });
    }

    // Info checks
    if let Some(fo_val) = fo {
        sub_checks.push(SubCheck {
            name: "fo".to_string(),
            verdict: Verdict::Info,
            detail: format!("fo={}", fo_val),
        });
    }

    if let Some(ri_val) = ri {
        sub_checks.push(SubCheck {
            name: "ri".to_string(),
            verdict: Verdict::Info,
            detail: format!("ri={}", ri_val),
        });
    }

    let detail = format!("DMARC: {}", record);
    let result = CheckResult::new(Category::Dmarc, sub_checks, detail);

    (result, policy, sub_policy, rua_external_auth_ok)
}

/// Extract domains from rua/ruf URIs like "mailto:reports@example.com,mailto:dmarc@third.example.com"
fn extract_report_domains(uri_list: &str) -> Vec<String> {
    uri_list
        .split(',')
        .filter_map(|uri| {
            let uri = uri.trim();
            if let Some(addr) = uri.strip_prefix("mailto:") {
                addr.split('@').nth(1).map(|d| d.to_lowercase())
            } else if uri.starts_with("https://") {
                // HTTPS report URI — extract domain
                uri.strip_prefix("https://")
                    .and_then(|s| s.split('/').next())
                    .map(|d| d.to_lowercase())
            } else {
                None
            }
        })
        .collect()
}
