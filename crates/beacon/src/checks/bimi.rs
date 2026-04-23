//! BIMI (Brand Indicators for Message Identification) check, per the BIMI
//! Working Group Draft (`draft-blank-ietf-bimi`).
//!
//! BIMI lets domain owners publish a brand logo reference for mail clients
//! to display alongside authenticated messages. This module resolves the
//! TXT record at `default._bimi.<domain>`, validates the `v=BIMI1` prefix,
//! and parses the `l=` (logo URL, required) and `a=` (Verified Mark
//! Certificate URL, optional) tags.
//!
//! The logo URL must be `https`; plain `http` is rejected with
//! `logo_http_not_allowed`. If a URL is present, the module attempts a
//! guarded fetch using a redirect-restricted client (`http_client_follow`)
//! that rejects SSRF-shaped redirects (non-HTTPS or private IPs), caps
//! response size, and classifies connection failures as
//! `timeout`/`tls`/`other` for the `beacon_upstream_errors_total` metric.
//! BIMI is advisory in the suite: most verdicts degrade to `Info`/`Warn`
//! rather than `Fail`, since DMARC enforcement (policy `quarantine` or
//! `reject`) is a prerequisite that the cross-validation step reports on
//! separately.

use crate::checks::util;
use crate::dns::DnsLookup;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Map a `reqwest::Error` to the coarse `kind` label for the
/// `beacon_upstream_errors_total` counter.
fn classify_reqwest_error(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "timeout"
    } else if e.is_connect() || e.to_string().to_lowercase().contains("tls") {
        "tls"
    } else {
        "other"
    }
}

fn record_upstream_error(kind: &'static str) {
    metrics::counter!(
        "beacon_upstream_errors_total",
        "backend" => "bimi_logo",
        "kind" => kind,
    )
    .increment(1);
}

/// Check BIMI at default._bimi.<domain>.
/// Returns (CheckResult, bimi_present).
#[tracing::instrument(skip_all, fields(category = "bimi", domain = %domain))]
pub async fn check_bimi(
    domain: &str,
    resolver: &impl DnsLookup,
    http_client_follow: &reqwest::Client,
) -> (CheckResult, bool) {
    let bimi_name = format!("default._bimi.{}", domain);
    let txt_records = resolver.lookup_txt(&bimi_name).await;

    let bimi_records: Vec<&str> = txt_records
        .iter()
        .filter(|t| t.starts_with("v=BIMI1"))
        .map(|s| s.as_str())
        .collect();

    let mut sub_checks = Vec::new();

    if bimi_records.is_empty() {
        sub_checks.push(SubCheck {
            name: "absent".to_string(),
            verdict: Verdict::Info,
            detail: "no BIMI record".to_string(),
        });
        let result = CheckResult::new(Category::Bimi, sub_checks, "No BIMI record".to_string());
        return (result, false);
    }

    let record = bimi_records[0];
    let tags = util::parse_tags(record);

    let logo_url = tags.get("l").cloned();
    let vmc_url = tags.get("a").cloned();

    match &logo_url {
        Some(url) if !url.is_empty() => {
            // Scheme guard: BIMI logo URL must use HTTPS
            let parsed = url::Url::parse(url).ok();
            if parsed.as_ref().map(|u| u.scheme() != "https").unwrap_or(true) {
                record_upstream_error("other");
                sub_checks.push(SubCheck {
                    name: "logo_http_not_allowed".to_string(),
                    verdict: Verdict::Fail,
                    detail: "BIMI logo URL must use HTTPS".to_string(),
                });
                let result = CheckResult::new(
                    Category::Bimi,
                    sub_checks,
                    "BIMI record found".to_string(),
                );
                return (result, true);
            }

            // SSRF check: resolve logo host and verify it's not a private address
            if let Some(logo_host) = extract_host(url) {
                let logo_ips = resolver.lookup_ips(&logo_host).await;
                let blocked = logo_ips
                    .iter()
                    .any(|ip| !netray_common::target_policy::is_allowed_target(*ip));
                if blocked {
                    record_upstream_error("ssrf_blocked");
                    sub_checks.push(SubCheck {
                        name: "logo_ssrf_blocked".to_string(),
                        verdict: Verdict::Fail,
                        detail: "BIMI logo URL resolves to a private address".to_string(),
                    });
                    let result = CheckResult::new(
                        Category::Bimi,
                        sub_checks,
                        "BIMI record found".to_string(),
                    );
                    return (result, true);
                }
            }

            // Check logo reachability via HEAD
            let fetch_start = std::time::Instant::now();
            match http_client_follow.head(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    // Post-redirect SSRF check
                    if let Some(final_host) = resp.url().host_str().map(|s| s.to_string()) {
                        let final_ips = resolver.lookup_ips(&final_host).await;
                        let redirect_blocked = final_ips
                            .iter()
                            .any(|ip| !netray_common::target_policy::is_allowed_target(*ip));
                        if redirect_blocked {
                            record_upstream_error("ssrf_blocked");
                            sub_checks.push(SubCheck {
                                name: "logo_redirect_ssrf_blocked".to_string(),
                                verdict: Verdict::Fail,
                                detail: "BIMI logo URL redirects to private address".to_string(),
                            });
                        } else {
                            sub_checks.push(SubCheck {
                                name: "logo_reachable".to_string(),
                                verdict: Verdict::Pass,
                                detail: format!("logo reachable at {}", url),
                            });
                        }
                    } else {
                        sub_checks.push(SubCheck {
                            name: "logo_reachable".to_string(),
                            verdict: Verdict::Pass,
                            detail: format!("logo reachable at {}", url),
                        });
                    }
                }
                Ok(resp) => {
                    let kind = match resp.status().as_u16() / 100 {
                        4 => "status_4xx",
                        5 => "status_5xx",
                        _ => "other",
                    };
                    record_upstream_error(kind);
                    sub_checks.push(SubCheck {
                        name: "logo_unreachable".to_string(),
                        verdict: Verdict::Warn,
                        detail: format!("logo unreachable: HTTP {}", resp.status()),
                    });
                }
                Err(e) => {
                    record_upstream_error(classify_reqwest_error(&e));
                    sub_checks.push(SubCheck {
                        name: "logo_unreachable".to_string(),
                        verdict: Verdict::Warn,
                        detail: format!("logo unreachable: {}", e),
                    });
                }
            }
            metrics::histogram!("beacon_https_fetch_duration_seconds", "target" => "bimi_logo")
                .record(fetch_start.elapsed().as_secs_f64());
        }
        _ => {
            sub_checks.push(SubCheck {
                name: "no_logo".to_string(),
                verdict: Verdict::Info,
                detail: "no logo URL (l=) in BIMI record".to_string(),
            });
        }
    }

    if let Some(url) = &vmc_url
        && !url.is_empty()
    {
        sub_checks.push(SubCheck {
            name: "vmc_present".to_string(),
            verdict: Verdict::Info,
            detail: "VMC certificate URL present; validation not performed".to_string(),
        });
    }

    let result = CheckResult::new(Category::Bimi, sub_checks, "BIMI record found".to_string());

    (result, true)
}

/// Extract hostname from a URL (https://host/path -> host).
fn extract_host(url: &str) -> Option<String> {
    let url = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = url.split('/').next()?;
    let host = host.split(':').next()?; // strip port
    if host.is_empty() {
        return None;
    }
    Some(host.to_string())
}
