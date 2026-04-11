use crate::checks::util;
use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, MtaStsInfo, SubCheck, Verdict};

/// Check MTA-STS for the domain.
/// Returns (CheckResult, Option<MtaStsInfo>).
pub async fn check_mta_sts(
    domain: &str,
    _mx_hosts: &[String],
    resolver: &DnsResolver,
    http_client: &reqwest::Client,
) -> (CheckResult, Option<MtaStsInfo>) {
    let mut sub_checks = Vec::new();

    // Step 1: DNS TXT record at _mta-sts.<domain>
    let sts_name = format!("_mta-sts.{}", domain);
    let txt_records = resolver.lookup_txt(&sts_name).await;

    let sts_records: Vec<&str> = txt_records
        .iter()
        .filter(|t| t.starts_with("v=STSv1"))
        .map(|s| s.as_str())
        .collect();

    if sts_records.is_empty() {
        sub_checks.push(SubCheck {
            name: "absent".to_string(),
            verdict: Verdict::Info,
            detail: "no MTA-STS DNS record".to_string(),
        });
        let result = CheckResult::new(
            Category::MtaSts,
            sub_checks,
            "No MTA-STS record".to_string(),
        );
        return (result, None);
    }

    let dns_record = sts_records[0];
    let dns_tags = util::parse_tags(dns_record);
    let dns_id = dns_tags.get("id").cloned().unwrap_or_default();

    // Step 2: Fetch HTTPS policy
    let policy_url = format!("https://mta-sts.{}/.well-known/mta-sts.txt", domain);

    // SSRF check: resolve mta-sts.<domain> and check for private IPs
    let sts_host = format!("mta-sts.{}", domain);
    let sts_ips = resolver.lookup_ips(&sts_host).await;
    for ip in &sts_ips {
        if !netray_common::target_policy::is_allowed_target(*ip) {
            sub_checks.push(SubCheck {
                name: "ssrf_blocked".to_string(),
                verdict: Verdict::Fail,
                detail: "MTA-STS policy host resolves to a private address".to_string(),
            });
            let info = MtaStsInfo {
                dns_id,
                policy_id: None,
                mode: None,
                mx_patterns: Vec::new(),
            };
            let result = CheckResult::new(
                Category::MtaSts,
                sub_checks,
                "MTA-STS fetch blocked".to_string(),
            );
            return (result, Some(info));
        }
    }

    let fetch_start = std::time::Instant::now();
    let response = match http_client.get(&policy_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            sub_checks.push(SubCheck {
                name: "https_fetch_failed".to_string(),
                verdict: Verdict::Fail,
                detail: format!("failed to fetch policy: {}", e),
            });
            let info = MtaStsInfo {
                dns_id,
                policy_id: None,
                mode: None,
                mx_patterns: Vec::new(),
            };
            let result = CheckResult::new(
                Category::MtaSts,
                sub_checks,
                "MTA-STS fetch failed".to_string(),
            );
            return (result, Some(info));
        }
    };

    metrics::histogram!("beacon_https_fetch_duration_seconds", "target" => "mta_sts")
        .record(fetch_start.elapsed().as_secs_f64());

    // Check for redirects (3xx)
    if response.status().is_redirection() {
        sub_checks.push(SubCheck {
            name: "https_redirect".to_string(),
            verdict: Verdict::Fail,
            detail: "policy endpoint must not redirect".to_string(),
        });
        let info = MtaStsInfo {
            dns_id,
            policy_id: None,
            mode: None,
            mx_patterns: Vec::new(),
        };
        let result = CheckResult::new(
            Category::MtaSts,
            sub_checks,
            "MTA-STS redirected".to_string(),
        );
        return (result, Some(info));
    }

    if !response.status().is_success() {
        sub_checks.push(SubCheck {
            name: "https_fetch_failed".to_string(),
            verdict: Verdict::Fail,
            detail: format!("HTTP {}", response.status()),
        });
        let info = MtaStsInfo {
            dns_id,
            policy_id: None,
            mode: None,
            mx_patterns: Vec::new(),
        };
        let result = CheckResult::new(
            Category::MtaSts,
            sub_checks,
            "MTA-STS fetch failed".to_string(),
        );
        return (result, Some(info));
    }

    // Check Content-Type
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let mime_type = content_type.split(';').next().unwrap_or("").trim();
    if mime_type != "text/plain" {
        sub_checks.push(SubCheck {
            name: "wrong_content_type".to_string(),
            verdict: Verdict::Fail,
            detail: format!("Content-Type '{}' is not text/plain", mime_type),
        });
    }

    let raw_bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            sub_checks.push(SubCheck {
                name: "https_fetch_failed".to_string(),
                verdict: Verdict::Fail,
                detail: format!("failed to read policy body: {}", e),
            });
            let info = MtaStsInfo {
                dns_id,
                policy_id: None,
                mode: None,
                mx_patterns: Vec::new(),
            };
            let result = CheckResult::new(
                Category::MtaSts,
                sub_checks,
                "MTA-STS body read failed".to_string(),
            );
            return (result, Some(info));
        }
    };

    let truncated = raw_bytes.len() > 65_536;
    let body_bytes = &raw_bytes[..raw_bytes.len().min(65_536)];
    let body = match std::str::from_utf8(body_bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            if truncated {
                sub_checks.push(SubCheck {
                    name: "body_truncated".to_string(),
                    verdict: Verdict::Warn,
                    detail: "policy body exceeds 64KB, truncated".to_string(),
                });
            }
            sub_checks.push(SubCheck {
                name: "https_fetch_failed".to_string(),
                verdict: Verdict::Fail,
                detail: "policy body is not valid UTF-8".to_string(),
            });
            let info = MtaStsInfo {
                dns_id,
                policy_id: None,
                mode: None,
                mx_patterns: Vec::new(),
            };
            let result = CheckResult::new(
                Category::MtaSts,
                sub_checks,
                "MTA-STS body read failed".to_string(),
            );
            return (result, Some(info));
        }
    };

    if truncated {
        sub_checks.push(SubCheck {
            name: "body_truncated".to_string(),
            verdict: Verdict::Warn,
            detail: "policy body exceeds 64KB, truncated".to_string(),
        });
    }

    // Parse policy — MTA-STS policy files do NOT contain an `id` field per RFC 8461.
    // The `id` is only in the DNS TXT record. However, some implementations include
    // it in the policy body, so we'll parse it if present for cross-validation.
    let mut policy_id = None;
    let mut mode = None;
    let mut max_age = None;
    let mut mx_patterns = Vec::new();

    for line in body.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim().to_string();
            match key.as_str() {
                "version" => {}
                "mode" => mode = Some(value),
                "max_age" => max_age = value.parse::<u64>().ok(),
                "mx" => mx_patterns.push(value),
                "id" => policy_id = Some(value),
                _ => {}
            }
        }
    }

    let info = MtaStsInfo {
        dns_id: dns_id.clone(),
        policy_id: policy_id.clone(),
        mode: mode.clone(),
        mx_patterns: mx_patterns.clone(),
    };

    // Mode check
    match mode.as_deref() {
        Some("enforce") => {
            sub_checks.push(SubCheck {
                name: "mode".to_string(),
                verdict: Verdict::Pass,
                detail: "mode: enforce".to_string(),
            });
        }
        Some("testing") => {
            sub_checks.push(SubCheck {
                name: "mode".to_string(),
                verdict: Verdict::Warn,
                detail: "mode: testing".to_string(),
            });
        }
        Some("none") => {
            sub_checks.push(SubCheck {
                name: "mode".to_string(),
                verdict: Verdict::Warn,
                detail: "mode: none".to_string(),
            });
        }
        Some(m) => {
            sub_checks.push(SubCheck {
                name: "mode".to_string(),
                verdict: Verdict::Warn,
                detail: format!("unknown mode: {}", m),
            });
        }
        None => {
            sub_checks.push(SubCheck {
                name: "mode".to_string(),
                verdict: Verdict::Fail,
                detail: "missing mode field".to_string(),
            });
        }
    }

    // Max-age check
    if let Some(age) = max_age
        && age < 86400
    {
        sub_checks.push(SubCheck {
            name: "max_age_low".to_string(),
            verdict: Verdict::Warn,
            detail: format!("max_age {} < 86400; suggest >= 604800", age),
        });
    }

    // MX coverage check is now performed in cross_validation.rs

    let detail = format!("MTA-STS id={}", dns_id);
    let result = CheckResult::new(Category::MtaSts, sub_checks, detail);

    (result, Some(info))
}
