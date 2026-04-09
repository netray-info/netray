use crate::checks::util;
use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Check BIMI at default._bimi.<domain>.
/// Returns (CheckResult, bimi_present).
pub async fn check_bimi(
    domain: &str,
    resolver: &DnsResolver,
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
        let result = CheckResult::new(
            Category::Bimi,
            sub_checks,
            "No BIMI record".to_string(),
        );
        return (result, false);
    }

    let record = bimi_records[0];
    let tags = util::parse_tags(record);

    let logo_url = tags.get("l").cloned();
    let vmc_url = tags.get("a").cloned();

    match &logo_url {
        Some(url) if !url.is_empty() => {
            // SSRF check: resolve logo host and verify it's not a private address
            if let Some(logo_host) = extract_host(url) {
                let logo_ips = resolver.lookup_ips(&logo_host).await;
                let blocked = logo_ips
                    .iter()
                    .any(|ip| !netray_common::target_policy::is_allowed_target(*ip));
                if blocked {
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
                    sub_checks.push(SubCheck {
                        name: "logo_unreachable".to_string(),
                        verdict: Verdict::Warn,
                        detail: format!("logo unreachable: HTTP {}", resp.status()),
                    });
                }
                Err(e) => {
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

    let result = CheckResult::new(
        Category::Bimi,
        sub_checks,
        "BIMI record found".to_string(),
    );

    (result, true)
}

/// Extract hostname from a URL (https://host/path -> host).
fn extract_host(url: &str) -> Option<String> {
    let url = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = url.split('/').next()?;
    let host = host.split(':').next()?; // strip port
    if host.is_empty() {
        return None;
    }
    Some(host.to_string())
}
