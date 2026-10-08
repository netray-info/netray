//! MTA-STS (SMTP MTA Strict Transport Security) check, per RFC 8461.
//!
//! MTA-STS advertises a TLS policy for inbound SMTP through two signals: a
//! `TXT` record at `_mta-sts.<domain>` announcing the policy ID, and a
//! policy file served over HTTPS at
//! `https://mta-sts.<domain>/.well-known/mta-sts.txt`. This module performs
//! both lookups, parses the policy file (`version`, `mode`, `mx:`, `max_age`
//! tags), and surfaces verdicts for record absence, policy-file fetch
//! errors, mode strictness (`enforce` vs `testing` vs `none`), MX-pattern
//! coverage, and `max_age` bounds.
//!
//! The policy is fetched through [`netray_common::fetch`]: HTTPS only, no
//! redirect followed per RFC 8461 §3.3 (a 3xx answer fails the check), every
//! resolved address checked by [`OutboundFetch::allow`], the response body
//! cut at 64 KB to prevent resource exhaustion. Parsed MX patterns are capped
//! at 32. Fetch failures are categorised as `timeout`/`tls`/`blocked`/`other`
//! for the `beacon_upstream_errors_total` metric.

use netray_common::fetch::{self, AtLimit, FetchError, FetchOptions};
use reqwest::header::HeaderMap;
use reqwest::{Method, StatusCode};

use crate::checks::util;
use crate::dns::DnsLookup;
use crate::quality::{Category, CheckResult, MtaStsInfo, SubCheck, Verdict};
use crate::state::OutboundFetch;

const MTA_STS_MAX_BODY_BYTES: usize = 65_536; // generous cap per RFC 8461; real policies are ~200 bytes
const MTA_STS_MAX_MX_PATTERNS: usize = 32;

/// Map a [`FetchError`] to the coarse `kind` label for the
/// `beacon_upstream_errors_total` counter.
fn classify_fetch_error(e: &FetchError) -> &'static str {
    match e {
        FetchError::Timeout => "timeout",
        FetchError::Blocked { .. } => "blocked",
        FetchError::Http(e) => {
            if e.is_timeout() {
                "timeout"
            } else if e.is_connect() || e.to_string().to_lowercase().contains("tls") {
                "tls"
            } else {
                "other"
            }
        }
        _ => "other",
    }
}

/// Fixed sub-check detail for a fetch error; never the error's own text.
fn fetch_error_detail(e: &FetchError) -> &'static str {
    match e {
        FetchError::Blocked { .. } => "policy host not reachable",
        FetchError::Timeout => "failed to fetch policy: timeout",
        FetchError::Http(e) if e.is_timeout() => "failed to fetch policy: timeout",
        FetchError::Http(e) if e.is_connect() => "failed to fetch policy: connection failed",
        _ => "failed to fetch policy",
    }
}

fn info_without_policy(dns_id: String) -> MtaStsInfo {
    MtaStsInfo {
        dns_id,
        policy_id: None,
        mode: None,
        mx_patterns: Vec::new(),
    }
}

fn record_upstream_error(backend: &'static str, kind: &'static str) {
    metrics::counter!(
        "beacon_upstream_errors_total",
        "backend" => backend,
        "kind" => kind,
    )
    .increment(1);
}

/// Check MTA-STS for the domain.
/// Returns (CheckResult, Option<MtaStsInfo>).
#[tracing::instrument(skip_all, fields(category = "mta_sts", domain = %domain))]
pub async fn check_mta_sts(
    domain: &str,
    resolver: &impl DnsLookup,
    fetch: &OutboundFetch,
) -> (CheckResult, Option<MtaStsInfo>) {
    let policy_url = format!("https://mta-sts.{}/.well-known/mta-sts.txt", domain);
    check_mta_sts_at(domain, resolver, fetch, &policy_url).await
}

/// [`check_mta_sts`] with the policy fetched from `policy_url`.
pub(crate) async fn check_mta_sts_at(
    domain: &str,
    resolver: &impl DnsLookup,
    fetch: &OutboundFetch,
    policy_url: &str,
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

    // Step 2: refuse a policy host with an address `allow` does not admit
    let sts_host = format!("mta-sts.{}", domain);
    let sts_ips = resolver.lookup_ips(&sts_host).await;
    if sts_ips.iter().any(|ip| !(fetch.allow)(*ip)) {
        record_upstream_error("mta_sts", "blocked");
        sub_checks.push(SubCheck {
            name: "ssrf_blocked".to_string(),
            verdict: Verdict::Fail,
            detail: "MTA-STS policy host resolves to a private address".to_string(),
        });
        let result = CheckResult::new(
            Category::MtaSts,
            sub_checks,
            "MTA-STS fetch blocked".to_string(),
        );
        return (result, Some(info_without_policy(dns_id)));
    }

    // Step 3: fetch and evaluate the HTTPS policy
    let (fetch_sub_checks, info, detail) = fetch_and_parse_policy(policy_url, dns_id, fetch).await;
    sub_checks.extend(fetch_sub_checks);
    let result = CheckResult::new(Category::MtaSts, sub_checks, detail);
    (result, info)
}

/// Fetch the MTA-STS policy from `policy_url` and evaluate it with
/// [`evaluate_policy`].
async fn fetch_and_parse_policy(
    policy_url: &str,
    dns_id: String,
    fetch: &OutboundFetch,
) -> (Vec<SubCheck>, Option<MtaStsInfo>, String) {
    let opts = FetchOptions {
        max_redirects: 0,
        at_limit: AtLimit::ReturnLast,
        https_only: true,
        body_cap: MTA_STS_MAX_BODY_BYTES,
        truncate_body: true,
        timeout: fetch.timeout,
        allow: fetch.allow,
        ..FetchOptions::new(Method::GET)
    };

    let fetch_start = std::time::Instant::now();
    match fetch::fetch(&fetch.settings, fetch.resolver.clone(), policy_url, &opts).await {
        Ok(r) => {
            metrics::histogram!("beacon_https_fetch_duration_seconds", "target" => "mta_sts")
                .record(fetch_start.elapsed().as_secs_f64());
            evaluate_policy(r.status, &r.headers, &r.body, r.body_truncated, dns_id)
        }
        Err(e) => {
            record_upstream_error("mta_sts", classify_fetch_error(&e));
            let sub_checks = vec![SubCheck {
                name: "https_fetch_failed".to_string(),
                verdict: Verdict::Fail,
                detail: fetch_error_detail(&e).to_string(),
            }];
            (
                sub_checks,
                Some(info_without_policy(dns_id)),
                "MTA-STS fetch failed".to_string(),
            )
        }
    }
}

/// Evaluate a fetched policy response: status, Content-Type, body size and
/// the policy fields. `truncated` means `body` was cut at
/// `MTA_STS_MAX_BODY_BYTES`.
fn evaluate_policy(
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
    truncated: bool,
    dns_id: String,
) -> (Vec<SubCheck>, Option<MtaStsInfo>, String) {
    let mut sub_checks = Vec::new();

    if status.is_redirection() {
        record_upstream_error("mta_sts", "other");
        sub_checks.push(SubCheck {
            name: "https_redirect".to_string(),
            verdict: Verdict::Fail,
            detail: "policy endpoint must not redirect".to_string(),
        });
        return (
            sub_checks,
            Some(info_without_policy(dns_id)),
            "MTA-STS redirected".to_string(),
        );
    }

    if !status.is_success() {
        let kind = match status.as_u16() / 100 {
            4 => "status_4xx",
            5 => "status_5xx",
            _ => "other",
        };
        record_upstream_error("mta_sts", kind);
        sub_checks.push(SubCheck {
            name: "https_fetch_failed".to_string(),
            verdict: Verdict::Fail,
            detail: format!("HTTP {}", status),
        });
        return (
            sub_checks,
            Some(info_without_policy(dns_id)),
            "MTA-STS fetch failed".to_string(),
        );
    }

    let content_type = headers
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

    if truncated {
        record_upstream_error("mta_sts", "size_cap");
        sub_checks.push(SubCheck {
            name: "policy_body_too_large".to_string(),
            verdict: Verdict::Warn,
            detail: format!("policy body exceeds {}B, truncated", MTA_STS_MAX_BODY_BYTES),
        });
    }

    let body = match std::str::from_utf8(body) {
        Ok(s) => s,
        Err(_) => {
            sub_checks.push(SubCheck {
                name: "https_fetch_failed".to_string(),
                verdict: Verdict::Fail,
                detail: "policy body is not valid UTF-8".to_string(),
            });
            return (
                sub_checks,
                Some(info_without_policy(dns_id)),
                "MTA-STS body read failed".to_string(),
            );
        }
    };

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

    if mx_patterns.len() > MTA_STS_MAX_MX_PATTERNS {
        mx_patterns.truncate(MTA_STS_MAX_MX_PATTERNS);
        sub_checks.push(SubCheck {
            name: "policy_mx_entries_truncated".to_string(),
            verdict: Verdict::Warn,
            detail: format!(
                "MTA-STS policy lists more than {} MX patterns; extras discarded",
                MTA_STS_MAX_MX_PATTERNS
            ),
        });
    }

    let info = MtaStsInfo {
        dns_id: dns_id.clone(),
        policy_id: policy_id.clone(),
        mode: mode.clone(),
        mx_patterns: mx_patterns.clone(),
    };

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

    if let Some(age) = max_age
        && age < 86400
    {
        sub_checks.push(SubCheck {
            name: "max_age_low".to_string(),
            verdict: Verdict::Warn,
            detail: format!("max_age {} < 86400; suggest >= 604800", age),
        });
    }

    let detail = format!("MTA-STS id={}", dns_id);
    (sub_checks, Some(info), detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::FetchResolver;
    use crate::dns::test_support::TestDnsResolver;
    use crate::state::OutboundFetch;
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use std::sync::Arc;

    /// Body exactly `MTA_STS_MAX_BODY_BYTES + 1` → policy_body_too_large Warn.
    #[tokio::test]
    async fn body_exceeds_cap_warns() {
        // Construct a valid policy preamble + filler to exceed the cap.
        let preamble = "version: STSv1\nmode: enforce\nmx: mail.example.com\nmax_age: 604800\n";
        let mut body = String::from(preamble);
        while body.len() <= MTA_STS_MAX_BODY_BYTES {
            body.push('x');
        }
        // Now body.len() == MTA_STS_MAX_BODY_BYTES + 1 (or slightly more; one more 'x' fine).
        assert!(body.len() > MTA_STS_MAX_BODY_BYTES);

        // The fetch truncates at the cap and reports it.
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        let (sub_checks, _info, _detail) = evaluate_policy(
            StatusCode::OK,
            &headers,
            &body.as_bytes()[..MTA_STS_MAX_BODY_BYTES],
            true,
            "20200101T000000".to_string(),
        );

        assert!(
            sub_checks
                .iter()
                .any(|s| s.name == "policy_body_too_large" && s.verdict == Verdict::Warn),
            "expected policy_body_too_large Warn; got {:?}",
            sub_checks
        );
    }

    /// Valid body with `mode=enforce` and `id` → Pass verdict on mode, info.policy_id set.
    #[tokio::test]
    async fn valid_enforce_policy_passes() {
        let body = "version: STSv1\nmode: enforce\nmx: mail.example.com\nmax_age: 604800\nid: 20200101T000000\n";
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("text/plain"));
        let (sub_checks, info, _detail) = evaluate_policy(
            StatusCode::OK,
            &headers,
            body.as_bytes(),
            false,
            "20200101T000000".to_string(),
        );

        assert!(
            sub_checks
                .iter()
                .any(|s| s.name == "mode" && s.verdict == Verdict::Pass),
            "expected mode Pass; got {:?}",
            sub_checks
        );
        let info = info.expect("MtaStsInfo");
        assert_eq!(info.policy_id.as_deref(), Some("20200101T000000"));
        assert_eq!(info.mode.as_deref(), Some("enforce"));
    }

    /// Wrong content-type → wrong_content_type Fail.
    #[tokio::test]
    async fn wrong_content_type_fails() {
        let body = "version: STSv1\nmode: enforce\nmx: mail.example.com\nmax_age: 604800\n";
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        let (sub_checks, _info, _detail) = evaluate_policy(
            StatusCode::OK,
            &headers,
            body.as_bytes(),
            false,
            String::new(),
        );

        assert!(
            sub_checks
                .iter()
                .any(|s| s.name == "wrong_content_type" && s.verdict == Verdict::Fail),
            "expected wrong_content_type Fail; got {:?}",
            sub_checks
        );
    }

    /// SSRF hostname → ssrf_blocked Fail (full check_mta_sts path).
    #[tokio::test]
    async fn ssrf_hostname_blocked() {
        let domain = "example.com";
        let resolver = Arc::new(
            TestDnsResolver::new()
                .with_txt("_mta-sts.example.com", vec!["v=STSv1; id=20200101T000000;"])
                .with_ips(
                    "mta-sts.example.com",
                    vec!["10.0.0.1".parse::<std::net::IpAddr>().unwrap()],
                ),
        );
        let fetch = OutboundFetch::new(5_000, Arc::new(FetchResolver(resolver.clone())));

        let (result, info) = check_mta_sts(domain, &*resolver, &fetch).await;
        assert!(
            result
                .sub_checks
                .iter()
                .any(|s| s.name == "ssrf_blocked" && s.verdict == Verdict::Fail),
            "expected ssrf_blocked Fail; got {:?}",
            result.sub_checks
        );
        let info = info.expect("MtaStsInfo on SSRF block");
        assert_eq!(info.dns_id, "20200101T000000");
    }
}
