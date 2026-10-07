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
//! The HTTP fetch uses a no-redirect client per RFC 8461 §3.3 (redirects
//! are explicitly disallowed for the policy endpoint), caps the response
//! body at 64 KB to prevent resource exhaustion, and caps parsed MX
//! patterns at 32. Connection-level failures are categorised as
//! `timeout`/`tls`/`other` for the `beacon_upstream_errors_total` metric.

use crate::checks::util;
use crate::dns::DnsLookup;
use crate::quality::{Category, CheckResult, MtaStsInfo, SubCheck, Verdict};

const MTA_STS_MAX_BODY_BYTES: usize = 65_536; // generous cap per RFC 8461; real policies are ~200 bytes
const MTA_STS_MAX_MX_PATTERNS: usize = 32;

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
            record_upstream_error("mta_sts", "ssrf_blocked");
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

    let (fetch_sub_checks, info, detail) =
        fetch_and_parse_policy(&policy_url, dns_id.clone(), http_client).await;
    sub_checks.extend(fetch_sub_checks);
    let result = CheckResult::new(Category::MtaSts, sub_checks, detail);
    (result, info)
}

/// Fetch the MTA-STS policy from `policy_url`, parse it, and return the
/// derived sub-checks, [`MtaStsInfo`], and detail string. This helper exists
/// so tests can exercise the fetch + parse logic against a local mock
/// server without tripping the SSRF pre-flight in [`check_mta_sts`].
async fn fetch_and_parse_policy(
    policy_url: &str,
    dns_id: String,
    http_client: &reqwest::Client,
) -> (Vec<SubCheck>, Option<MtaStsInfo>, String) {
    let mut sub_checks = Vec::new();

    let fetch_start = std::time::Instant::now();
    let response = match http_client.get(policy_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            record_upstream_error("mta_sts", classify_reqwest_error(&e));
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
            return (sub_checks, Some(info), "MTA-STS fetch failed".to_string());
        }
    };

    metrics::histogram!("beacon_https_fetch_duration_seconds", "target" => "mta_sts")
        .record(fetch_start.elapsed().as_secs_f64());

    if response.status().is_redirection() {
        record_upstream_error("mta_sts", "other");
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
        return (sub_checks, Some(info), "MTA-STS redirected".to_string());
    }

    if !response.status().is_success() {
        let kind = match response.status().as_u16() / 100 {
            4 => "status_4xx",
            5 => "status_5xx",
            _ => "other",
        };
        record_upstream_error("mta_sts", kind);
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
        return (sub_checks, Some(info), "MTA-STS fetch failed".to_string());
    }

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

    let mut body_bytes: Vec<u8> = Vec::with_capacity(4096);
    let mut truncated = false;
    let mut response = response;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body_bytes.len() + chunk.len() > MTA_STS_MAX_BODY_BYTES {
                    let remaining = MTA_STS_MAX_BODY_BYTES - body_bytes.len();
                    body_bytes.extend_from_slice(&chunk[..remaining]);
                    truncated = true;
                    break;
                }
                body_bytes.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(e) => {
                record_upstream_error("mta_sts", classify_reqwest_error(&e));
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
                return (
                    sub_checks,
                    Some(info),
                    "MTA-STS body read failed".to_string(),
                );
            }
        }
    }

    if truncated {
        record_upstream_error("mta_sts", "size_cap");
        sub_checks.push(SubCheck {
            name: "policy_body_too_large".to_string(),
            verdict: Verdict::Warn,
            detail: format!("policy body exceeds {}B, truncated", MTA_STS_MAX_BODY_BYTES),
        });
    }

    let body = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
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
            return (
                sub_checks,
                Some(info),
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
    use crate::dns::test_support::TestDnsResolver;
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use axum::routing::get;

    /// Bind an ephemeral loopback TCP listener and serve the provided `axum::Router`.
    /// Returns the base URL (e.g. `http://127.0.0.1:54321`) and the server's `JoinHandle`.
    async fn start_mock_server(handler: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move { axum::serve(listener, handler).await.unwrap() });
        (format!("http://127.0.0.1:{}", addr.port()), handle)
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }

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

        let body_clone = body.clone();
        let router = axum::Router::new().route(
            "/.well-known/mta-sts.txt",
            get(move || {
                let b = body_clone.clone();
                async move {
                    let mut headers = HeaderMap::new();
                    headers.insert(
                        "content-type",
                        HeaderValue::from_static("text/plain; charset=utf-8"),
                    );
                    (StatusCode::OK, headers, b)
                }
            }),
        );
        let (base, handle) = start_mock_server(router).await;
        let url = format!("{}/.well-known/mta-sts.txt", base);

        let (sub_checks, _info, _detail) =
            fetch_and_parse_policy(&url, "20200101T000000".to_string(), &client()).await;
        handle.abort();

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
        let router = axum::Router::new().route(
            "/.well-known/mta-sts.txt",
            get(move || async move {
                let mut headers = HeaderMap::new();
                headers.insert("content-type", HeaderValue::from_static("text/plain"));
                (StatusCode::OK, headers, body)
            }),
        );
        let (base, handle) = start_mock_server(router).await;
        let url = format!("{}/.well-known/mta-sts.txt", base);

        let (sub_checks, info, _detail) =
            fetch_and_parse_policy(&url, "20200101T000000".to_string(), &client()).await;
        handle.abort();

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
        let router = axum::Router::new().route(
            "/.well-known/mta-sts.txt",
            get(move || async move {
                let mut headers = HeaderMap::new();
                headers.insert("content-type", HeaderValue::from_static("application/json"));
                (StatusCode::OK, headers, body)
            }),
        );
        let (base, handle) = start_mock_server(router).await;
        let url = format!("{}/.well-known/mta-sts.txt", base);

        let (sub_checks, _info, _detail) =
            fetch_and_parse_policy(&url, String::new(), &client()).await;
        handle.abort();

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
        let resolver = TestDnsResolver::new()
            .with_txt("_mta-sts.example.com", vec!["v=STSv1; id=20200101T000000;"])
            .with_ips(
                "mta-sts.example.com",
                vec!["10.0.0.1".parse::<std::net::IpAddr>().unwrap()],
            );

        let (result, info) = check_mta_sts(domain, &resolver, &client()).await;
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
