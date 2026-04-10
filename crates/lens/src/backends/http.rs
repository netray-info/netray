use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;
use tracing::Instrument;

use crate::backends::{Backend, BackendContext, BackendExtra, BackendResult};
use crate::check::SectionError;
use crate::error::AppError;
use crate::scoring::engine::{CheckResult, CheckVerdict};

// ---------------------------------------------------------------------------
// Deserialization types for spectra's InspectResponse
//
// We only decode what we need; unknown fields are ignored via `#[serde(default)]`.
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
enum HttpCheckStatus {
    Pass,
    #[default]
    Skip,
    Warn,
    Fail,
}

impl HttpCheckStatus {
    fn to_verdict(self) -> CheckVerdict {
        match self {
            Self::Pass => CheckVerdict::Pass,
            Self::Skip => CheckVerdict::Skip,
            Self::Warn => CheckVerdict::Warn,
            Self::Fail => CheckVerdict::Fail,
        }
    }

    /// Worst-verdict ranking: Pass(0) < Skip(1) < Warn(2) < Fail(3).
    fn rank(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Skip => 1,
            Self::Warn => 2,
            Self::Fail => 3,
        }
    }
}

#[derive(Deserialize, Default)]
struct HttpInspectResponse {
    #[serde(default)]
    http_upgrade: Option<HttpUpgrade>,
    #[serde(default)]
    quality: QualityReport,
}

#[derive(Deserialize)]
struct HttpUpgrade {
    redirects_to_https: bool,
}

#[derive(Deserialize, Default)]
struct QualityReport {
    #[serde(default)]
    checks: Vec<QualityCheck>,
}

#[derive(Deserialize)]
struct QualityCheck {
    name: String,
    status: HttpCheckStatus,
    #[serde(default)]
    message: Option<String>,
}

// ---------------------------------------------------------------------------
// Backend trait implementation
// ---------------------------------------------------------------------------

pub struct HttpBackend {
    pub http_url: String,
}

impl Backend for HttpBackend {
    fn section(&self) -> &'static str {
        "http"
    }

    fn run(
        &self,
        client: &reqwest::Client,
        domain: &str,
        _context: &BackendContext,
        timeout: Duration,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BackendResult, SectionError>> + Send + '_>,
    > {
        let client = client.clone();
        let domain = domain.to_string();
        let http_url = self.http_url.clone();
        Box::pin(async move {
            check_http(&client, &http_url, &domain, timeout)
                .await
                .map_err(|e| match e {
                    AppError::Timeout => SectionError::Timeout,
                    other => SectionError::BackendError(other.to_string()),
                })
        })
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn check_http(
    client: &reqwest::Client,
    http_url: &str,
    domain: &str,
    timeout: Duration,
) -> Result<BackendResult, AppError> {
    let encoded_domain = percent_encode(domain);
    let url = format!(
        "{}/api/inspect?url=https%3A%2F%2F{}",
        http_url.trim_end_matches('/'),
        encoded_domain,
    );

    let span = tracing::info_span!("backend_call", service = "spectra", url = %url);
    check_http_inner(client, &url, http_url, timeout, &encoded_domain)
        .instrument(span)
        .await
}

async fn check_http_inner(
    client: &reqwest::Client,
    url: &str,
    http_url: &str,
    timeout: Duration,
    encoded_domain: &str,
) -> Result<BackendResult, AppError> {
    let resp = tokio::time::timeout(timeout, client.get(url).send())
        .await
        .map_err(|_| {
            tracing::warn!(service = "spectra", url = %url, error = "timeout", "backend call failed");
            AppError::Timeout
        })?
        .map_err(|e| {
            tracing::warn!(service = "spectra", url = %url, error = %e, "backend call failed");
            AppError::BackendError {
                backend: "http",
                message: e.to_string(),
            }
        })?;

    if !resp.status().is_success() {
        tracing::warn!(service = "spectra", url = %url, status = %resp.status(), "backend call failed");
        return Err(AppError::BackendError {
            backend: "http",
            message: format!("spectra returned HTTP {}", resp.status()),
        });
    }

    let inspect: HttpInspectResponse = resp.json().await.map_err(|e| {
        tracing::warn!(service = "spectra", url = %url, error = %e, "backend call failed");
        AppError::BackendError {
            backend: "http",
            message: format!("failed to decode spectra response: {e}"),
        }
    })?;

    tracing::debug!(service = "spectra", url = %url, "backend call succeeded");
    Ok(parse_inspect(inspect, http_url, encoded_domain))
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse_inspect(
    resp: HttpInspectResponse,
    http_url: &str,
    encoded_domain: &str,
) -> BackendResult {
    let checks_map: HashMap<&str, HttpCheckStatus> =
        resp.quality.checks.iter().map(|c| (c.name.as_str(), c.status)).collect();

    // https_redirect: synthesised from http_upgrade field.
    let https_redirect_status = match &resp.http_upgrade {
        None => HttpCheckStatus::Skip,
        Some(u) => {
            if u.redirects_to_https {
                HttpCheckStatus::Pass
            } else {
                HttpCheckStatus::Fail
            }
        }
    };

    // Direct lookups.
    let hsts_status = lookup_check(&resp.quality.checks, "hsts");
    let cors_status = lookup_check(&resp.quality.checks, "cors");
    let cookie_secure_status = lookup_check(&resp.quality.checks, "cookie_secure");

    // Aggregated: security_headers.
    let security_headers_status = aggregate_worst(&[
        *checks_map.get("csp").unwrap_or(&HttpCheckStatus::Skip),
        *checks_map.get("x_frame_options").unwrap_or(&HttpCheckStatus::Skip),
        *checks_map.get("x_content_type_options").unwrap_or(&HttpCheckStatus::Skip),
        *checks_map.get("referrer_policy").unwrap_or(&HttpCheckStatus::Skip),
        *checks_map.get("permissions_policy").unwrap_or(&HttpCheckStatus::Skip),
    ]);

    // Aggregated: hygiene.
    let hygiene_status = aggregate_worst(&[
        *checks_map.get("deprecated_headers").unwrap_or(&HttpCheckStatus::Skip),
        *checks_map.get("info_leakage").unwrap_or(&HttpCheckStatus::Skip),
        *checks_map.get("caching").unwrap_or(&HttpCheckStatus::Skip),
        *checks_map.get("redirect_limit").unwrap_or(&HttpCheckStatus::Skip),
    ]);

    let checks = vec![
        CheckResult {
            name: "https_redirect".to_string(),
            verdict: https_redirect_status.to_verdict(),
            messages: vec![],
        },
        CheckResult {
            name: "hsts".to_string(),
            verdict: hsts_status.to_verdict(),
            messages: check_messages(&resp.quality.checks, "hsts", hsts_status),
        },
        CheckResult {
            name: "security_headers".to_string(),
            verdict: security_headers_status.to_verdict(),
            messages: vec![],
        },
        CheckResult {
            name: "cors".to_string(),
            verdict: cors_status.to_verdict(),
            messages: check_messages(&resp.quality.checks, "cors", cors_status),
        },
        CheckResult {
            name: "cookie_secure".to_string(),
            verdict: cookie_secure_status.to_verdict(),
            messages: check_messages(&resp.quality.checks, "cookie_secure", cookie_secure_status),
        },
        CheckResult {
            name: "hygiene".to_string(),
            verdict: hygiene_status.to_verdict(),
            messages: vec![],
        },
    ];

    // Headline uses individual csp check, not aggregated security_headers.
    let csp_status = *checks_map.get("csp").unwrap_or(&HttpCheckStatus::Skip);
    let raw_headline = format!(
        "HTTPS {}  HSTS {}  CSP {}  CORS {}",
        verdict_symbol(https_redirect_status),
        verdict_symbol(hsts_status),
        verdict_symbol(csp_status),
        verdict_symbol(cors_status),
    );

    let detail_url = format!(
        "{}/?url=https%3A%2F%2F{}",
        http_url.trim_end_matches('/'),
        encoded_domain,
    );

    BackendResult {
        checks,
        extra: BackendExtra::Http {
            raw_headline,
            detail_url,
        },
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn aggregate_worst(statuses: &[HttpCheckStatus]) -> HttpCheckStatus {
    statuses
        .iter()
        .copied()
        .max_by_key(|s| s.rank())
        .unwrap_or(HttpCheckStatus::Skip)
}

fn lookup_check(checks: &[QualityCheck], name: &str) -> HttpCheckStatus {
    checks
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.status)
        .unwrap_or(HttpCheckStatus::Skip)
}

/// Return messages for a check — only for non-pass, non-skip verdicts.
fn check_messages(checks: &[QualityCheck], name: &str, status: HttpCheckStatus) -> Vec<String> {
    match status {
        HttpCheckStatus::Pass | HttpCheckStatus::Skip => vec![],
        _ => checks
            .iter()
            .find(|c| c.name == name)
            .and_then(|c| c.message.clone())
            .map(|m| vec![m])
            .unwrap_or_default(),
    }
}

fn verdict_symbol(s: HttpCheckStatus) -> &'static str {
    match s {
        HttpCheckStatus::Pass => "✓",
        HttpCheckStatus::Fail => "✗",
        HttpCheckStatus::Warn => "~",
        HttpCheckStatus::Skip => "-",
    }
}

/// Minimal percent-encoding for query string values.
/// Copied locally: only two callers across the codebase (http.rs, tls.rs) — rule of three not yet met.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_check(name: &str, status: HttpCheckStatus) -> QualityCheck {
        QualityCheck { name: name.to_string(), status, message: None }
    }

    // AC-1: URL construction
    #[test]
    fn url_construction() {
        let encoded = percent_encode("example.com");
        assert_eq!(encoded, "example.com");
        let url = format!(
            "{}/api/inspect?url=https%3A%2F%2F{}",
            "http://spectra:3000",
            encoded,
        );
        assert_eq!(url, "http://spectra:3000/api/inspect?url=https%3A%2F%2Fexample.com");
    }

    // AC-2: six checks always present
    #[test]
    fn six_checks_always_present() {
        let resp = HttpInspectResponse {
            http_upgrade: Some(HttpUpgrade { redirects_to_https: true }),
            quality: QualityReport {
                checks: vec![
                    make_check("hsts", HttpCheckStatus::Pass),
                    make_check("csp", HttpCheckStatus::Pass),
                    make_check("x_frame_options", HttpCheckStatus::Pass),
                    make_check("x_content_type_options", HttpCheckStatus::Pass),
                    make_check("referrer_policy", HttpCheckStatus::Pass),
                    make_check("permissions_policy", HttpCheckStatus::Pass),
                    make_check("cors", HttpCheckStatus::Pass),
                    make_check("cookie_secure", HttpCheckStatus::Pass),
                    make_check("deprecated_headers", HttpCheckStatus::Pass),
                    make_check("info_leakage", HttpCheckStatus::Pass),
                    make_check("caching", HttpCheckStatus::Pass),
                ],
            },
        };
        let result = parse_inspect(resp, "http://spectra:3000", "example.com");
        assert_eq!(result.checks.len(), 6);
        let names: Vec<&str> = result.checks.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"https_redirect"));
        assert!(names.contains(&"hsts"));
        assert!(names.contains(&"security_headers"));
        assert!(names.contains(&"cors"));
        assert!(names.contains(&"cookie_secure"));
        assert!(names.contains(&"hygiene"));
    }

    // AC-3: https_redirect synthesis
    #[test]
    fn https_redirect_null_is_skip() {
        let resp = HttpInspectResponse { http_upgrade: None, quality: QualityReport::default() };
        let result = parse_inspect(resp, "http://spectra:3000", "example.com");
        let c = result.checks.iter().find(|c| c.name == "https_redirect").unwrap();
        assert_eq!(c.verdict, CheckVerdict::Skip);
    }

    #[test]
    fn https_redirect_false_is_fail() {
        let resp = HttpInspectResponse {
            http_upgrade: Some(HttpUpgrade { redirects_to_https: false }),
            quality: QualityReport::default(),
        };
        let result = parse_inspect(resp, "http://spectra:3000", "example.com");
        let c = result.checks.iter().find(|c| c.name == "https_redirect").unwrap();
        assert_eq!(c.verdict, CheckVerdict::Fail);
    }

    #[test]
    fn https_redirect_true_is_pass() {
        let resp = HttpInspectResponse {
            http_upgrade: Some(HttpUpgrade { redirects_to_https: true }),
            quality: QualityReport::default(),
        };
        let result = parse_inspect(resp, "http://spectra:3000", "example.com");
        let c = result.checks.iter().find(|c| c.name == "https_redirect").unwrap();
        assert_eq!(c.verdict, CheckVerdict::Pass);
    }

    // AC-4: security_headers worst-verdict aggregation
    #[test]
    fn security_headers_worst_verdict_is_fail() {
        let resp = HttpInspectResponse {
            http_upgrade: None,
            quality: QualityReport {
                checks: vec![
                    make_check("csp", HttpCheckStatus::Pass),
                    make_check("x_frame_options", HttpCheckStatus::Warn),
                    make_check("x_content_type_options", HttpCheckStatus::Pass),
                    make_check("referrer_policy", HttpCheckStatus::Fail),
                    make_check("permissions_policy", HttpCheckStatus::Skip),
                ],
            },
        };
        let result = parse_inspect(resp, "http://spectra:3000", "example.com");
        let c = result.checks.iter().find(|c| c.name == "security_headers").unwrap();
        assert_eq!(c.verdict, CheckVerdict::Fail);
    }

    // AC-5: hygiene aggregation with absent checks
    #[test]
    fn hygiene_aggregation_absent_checks_is_warn() {
        let resp = HttpInspectResponse {
            http_upgrade: None,
            quality: QualityReport {
                checks: vec![
                    make_check("deprecated_headers", HttpCheckStatus::Warn),
                    make_check("info_leakage", HttpCheckStatus::Skip),
                    // caching and redirect_limit absent
                ],
            },
        };
        let result = parse_inspect(resp, "http://spectra:3000", "example.com");
        let c = result.checks.iter().find(|c| c.name == "hygiene").unwrap();
        assert_eq!(c.verdict, CheckVerdict::Warn);
    }

    // AC-10: unknown fields ignored (Deserialize with extra fields)
    #[test]
    fn unknown_fields_deserialize_ok() {
        let json = r#"{
            "http_upgrade": {"redirects_to_https": true, "unknown_field": 42},
            "quality": {"checks": [{"name": "hsts", "status": "pass", "extra": "ignored"}]},
            "another_unknown": "value"
        }"#;
        let resp: HttpInspectResponse = serde_json::from_str(json).unwrap();
        assert!(resp.http_upgrade.as_ref().unwrap().redirects_to_https);
        assert_eq!(resp.quality.checks[0].status, HttpCheckStatus::Pass);
    }

    // AC-11: raw_headline format
    #[test]
    fn raw_headline_format() {
        let resp = HttpInspectResponse {
            http_upgrade: Some(HttpUpgrade { redirects_to_https: true }),
            quality: QualityReport {
                checks: vec![
                    make_check("hsts", HttpCheckStatus::Pass),
                    make_check("csp", HttpCheckStatus::Fail),
                    make_check("cors", HttpCheckStatus::Pass),
                ],
            },
        };
        let result = parse_inspect(resp, "http://spectra:3000", "example.com");
        let BackendExtra::Http { raw_headline, .. } = &result.extra else {
            panic!("expected Http extra");
        };
        assert_eq!(raw_headline, "HTTPS ✓  HSTS ✓  CSP ✗  CORS ✓");
    }

    // detail_url format
    #[test]
    fn detail_url_format() {
        let resp = HttpInspectResponse::default();
        let result = parse_inspect(resp, "http://spectra:3000", "example.com");
        let BackendExtra::Http { detail_url, .. } = &result.extra else {
            panic!("expected Http extra");
        };
        assert_eq!(detail_url, "http://spectra:3000/?url=https%3A%2F%2Fexample.com");
    }
}
