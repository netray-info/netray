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
//! `logo_http_not_allowed`. If a URL is present, the module sends a HEAD
//! request through [`netray_common::fetch`]: HTTPS only on every hop, at most
//! four redirects, every resolved address and IP literal checked by
//! [`OutboundFetch::allow`] before a connection is made. Failures are
//! classified as `timeout`/`tls`/`blocked`/`other` for the
//! `beacon_upstream_errors_total` metric. BIMI is advisory in the suite: most verdicts degrade to `Info`/`Warn`
//! rather than `Fail`, since DMARC enforcement (policy `quarantine` or
//! `reject`) is a prerequisite that the cross-validation step reports on
//! separately.

use netray_common::fetch::{self, AtLimit, BlockReason, FetchError, FetchOptions};
use reqwest::Method;

use crate::checks::util;
use crate::dns::DnsLookup;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};
use crate::state::OutboundFetch;

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

/// Sub-check for a failed logo fetch. Details are fixed texts, never the
/// error's own text.
fn fetch_error_sub_check(e: &FetchError, initial_is_name: bool) -> SubCheck {
    let (name, verdict, detail) = match e {
        FetchError::Blocked { hop, .. } if *hop >= 1 => (
            "logo_redirect_ssrf_blocked",
            Verdict::Fail,
            "BIMI logo URL redirects to private address",
        ),
        FetchError::Blocked {
            reason: BlockReason::Disallowed,
            ..
        } if initial_is_name => (
            "logo_ssrf_blocked",
            Verdict::Fail,
            "BIMI logo URL resolves to a private address",
        ),
        FetchError::Blocked { .. } => {
            ("logo_unreachable", Verdict::Warn, "logo host not reachable")
        }
        FetchError::TooManyRedirects => (
            "logo_unreachable",
            Verdict::Warn,
            "logo unreachable: too many redirects",
        ),
        FetchError::Scheme(_) => (
            "logo_unreachable",
            Verdict::Warn,
            "logo unreachable: redirect to non-HTTPS URL",
        ),
        FetchError::Timeout => (
            "logo_unreachable",
            Verdict::Warn,
            "logo unreachable: timeout",
        ),
        FetchError::Http(e) if e.is_timeout() => (
            "logo_unreachable",
            Verdict::Warn,
            "logo unreachable: timeout",
        ),
        FetchError::Http(e) if e.is_connect() => (
            "logo_unreachable",
            Verdict::Warn,
            "logo unreachable: connection failed",
        ),
        FetchError::Http(_) => (
            "logo_unreachable",
            Verdict::Warn,
            "logo unreachable: request failed",
        ),
        FetchError::InvalidUrl(_) => (
            "logo_unreachable",
            Verdict::Warn,
            "logo unreachable: invalid URL",
        ),
        FetchError::BodyTooLarge => (
            "logo_unreachable",
            Verdict::Warn,
            "logo unreachable: response too large",
        ),
    };
    SubCheck {
        name: name.to_string(),
        verdict,
        detail: detail.to_string(),
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
    fetch: &OutboundFetch,
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
            let Some(parsed) = parsed.filter(|u| u.scheme() == "https") else {
                record_upstream_error("other");
                sub_checks.push(SubCheck {
                    name: "logo_http_not_allowed".to_string(),
                    verdict: Verdict::Fail,
                    detail: "BIMI logo URL must use HTTPS".to_string(),
                });
                let result =
                    CheckResult::new(Category::Bimi, sub_checks, "BIMI record found".to_string());
                return (result, true);
            };
            let initial_is_name = matches!(parsed.host(), Some(url::Host::Domain(_)));

            // Check logo reachability via HEAD
            let opts = FetchOptions {
                max_redirects: 4,
                at_limit: AtLimit::Fail,
                https_only: true,
                read_body: false,
                timeout: fetch.timeout,
                allow: fetch.allow,
                ..FetchOptions::new(Method::HEAD)
            };
            let fetch_start = std::time::Instant::now();
            match fetch::fetch(&fetch.settings, fetch.resolver.clone(), url, &opts).await {
                Ok(resp) if resp.status.is_success() => {
                    sub_checks.push(SubCheck {
                        name: "logo_reachable".to_string(),
                        verdict: Verdict::Pass,
                        detail: format!("logo reachable at {}", url),
                    });
                }
                Ok(resp) => {
                    let kind = match resp.status.as_u16() / 100 {
                        4 => "status_4xx",
                        5 => "status_5xx",
                        _ => "other",
                    };
                    record_upstream_error(kind);
                    sub_checks.push(SubCheck {
                        name: "logo_unreachable".to_string(),
                        verdict: Verdict::Warn,
                        detail: format!("logo unreachable: HTTP {}", resp.status),
                    });
                }
                Err(e) => {
                    record_upstream_error(classify_fetch_error(&e));
                    sub_checks.push(fetch_error_sub_check(&e, initial_is_name));
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
