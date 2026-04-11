use std::time::Duration;

use serde::Deserialize;
use tracing::Instrument;

use crate::backends::{Backend, BackendContext, BackendExtra, BackendResult};
use crate::check::SectionError;
use crate::error::AppError;
use crate::scoring::engine::{CheckResult, CheckVerdict};

// ---------------------------------------------------------------------------
// Public result type
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct TlsBackendResult {
    pub checks: Vec<CheckResult>,
    pub raw_headline: String,
    pub detail_url: String,
}

// ---------------------------------------------------------------------------
// Deserialization types for tlsight's InspectResponse
//
// We only decode what we need; unknown fields are ignored via `#[serde(default)]`.
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct InspectResponse {
    #[serde(default)]
    ports: Vec<PortResult>,
    #[serde(default)]
    quality: Option<QualityResult>,
}

#[derive(Deserialize)]
struct PortResult {
    #[serde(default)]
    ips: Vec<IpResult>,
    quality: Option<PortQualityResult>,
}

#[derive(Deserialize, Default)]
struct PortQualityResult {
    #[serde(default)]
    checks: Vec<HealthCheck>,
}

#[derive(Deserialize, Default)]
struct IpResult {
    tls: Option<TlsParams>,
    chain: Option<Vec<CertInfo>>,
}

#[derive(Deserialize)]
struct TlsParams {
    version: String,
}

#[derive(Deserialize)]
struct CertInfo {
    days_remaining: Option<i64>,
}

#[derive(Deserialize, Default)]
struct QualityResult {
    #[serde(default)]
    checks: Vec<HealthCheck>,
}

#[derive(Deserialize)]
struct HealthCheck {
    id: String,
    status: String,
    #[serde(default)]
    detail: Option<String>,
}

// ---------------------------------------------------------------------------
// Backend trait implementation
// ---------------------------------------------------------------------------

pub struct TlsBackend {
    pub tls_url: String,
    pub public_url: String,
}

impl Backend for TlsBackend {
    fn section(&self) -> &'static str {
        "tls"
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
        let tls_url = self.tls_url.clone();
        let public_url = self.public_url.clone();
        Box::pin(async move {
            let mut result = check_tls(&client, &tls_url, &domain, timeout)
                .await
                .map_err(|e| match e {
                    AppError::Timeout => SectionError::Timeout,
                    other => SectionError::BackendError(other.to_string()),
                })?;
            result.detail_url = format!(
                "{}/?h={}",
                public_url.trim_end_matches('/'),
                percent_encode(&domain),
            );
            Ok(BackendResult {
                checks: result.checks,
                extra: BackendExtra::Tls {
                    raw_headline: result.raw_headline,
                    detail_url: result.detail_url,
                },
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn check_tls(
    client: &reqwest::Client,
    tls_url: &str,
    domain: &str,
    timeout: Duration,
) -> Result<TlsBackendResult, AppError> {
    let url = format!(
        "{}/api/inspect?h={}",
        tls_url.trim_end_matches('/'),
        percent_encode(domain),
    );

    let span = tracing::info_span!("backend_call", service = "tlsight", url = %url);
    check_tls_inner(client, &url, domain, tls_url, timeout)
        .instrument(span)
        .await
}

async fn check_tls_inner(
    client: &reqwest::Client,
    url: &str,
    domain: &str,
    tls_url: &str,
    timeout: Duration,
) -> Result<TlsBackendResult, AppError> {
    let resp = tokio::time::timeout(timeout, client.get(url).send())
        .await
        .map_err(|_| {
            tracing::warn!(service = "tlsight", url = %url, error = "timeout", "backend call failed");
            AppError::Timeout
        })?
        .map_err(|e| {
            tracing::warn!(service = "tlsight", url = %url, error = %e, "backend call failed");
            AppError::BackendError {
                backend: "tls",
                message: e.to_string(),
            }
        })?;

    if !resp.status().is_success() {
        tracing::warn!(service = "tlsight", url = %url, status = %resp.status(), "backend call failed");
        return Err(AppError::BackendError {
            backend: "tls",
            message: format!("tlsight returned HTTP {}", resp.status()),
        });
    }

    let inspect: InspectResponse = resp.json().await.map_err(|e| {
        tracing::warn!(service = "tlsight", url = %url, error = %e, "backend call failed");
        AppError::BackendError {
            backend: "tls",
            message: format!("failed to decode tlsight response: {e}"),
        }
    })?;

    tracing::debug!(service = "tlsight", url = %url, "backend call succeeded");
    Ok(parse_inspect(inspect, domain, tls_url))
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse_inspect(inspect: InspectResponse, domain: &str, tls_url: &str) -> TlsBackendResult {
    let mut checks: Vec<CheckResult> = Vec::new();

    let quality = inspect.quality.unwrap_or_default();

    // Collect per-port quality checks (cert, protocol, config) from the first port.
    // These are in ports[0].quality.checks (PortQualityResult).
    if let Some(port_quality) = inspect.ports.first().and_then(|p| p.quality.as_ref()) {
        for hc in &port_quality.checks {
            let verdict = match hc.status.as_str() {
                "pass" => CheckVerdict::Pass,
                "warn" => CheckVerdict::Warn,
                "fail" => CheckVerdict::Fail,
                "skip" => CheckVerdict::Skip,
                _ => CheckVerdict::Skip,
            };
            let messages = tls_check_messages(&verdict, &hc.detail);
            checks.push(CheckResult {
                name: hc.id.clone(),
                verdict,
                messages,
            });
        }
    }

    // Collect hostname-scoped quality checks (hsts, https_redirect) from top-level quality.checks.
    for hc in &quality.checks {
        if checks.iter().any(|c| c.name == hc.id) {
            continue;
        }
        let verdict = match hc.status.as_str() {
            "pass" => CheckVerdict::Pass,
            "warn" => CheckVerdict::Warn,
            "fail" => CheckVerdict::Fail,
            "skip" => CheckVerdict::Skip,
            _ => CheckVerdict::Skip,
        };
        let messages = tls_check_messages(&verdict, &hc.detail);
        checks.push(CheckResult {
            name: hc.id.clone(),
            verdict,
            messages,
        });
    }

    let raw_headline = build_headline(&inspect.ports);
    let detail_url = format!(
        "{}/?h={}",
        tls_url.trim_end_matches('/'),
        percent_encode(domain),
    );

    TlsBackendResult {
        checks,
        raw_headline,
        detail_url,
    }
}

/// Return diagnostic messages for a TLS check — only for non-passing verdicts.
fn tls_check_messages(verdict: &CheckVerdict, detail: &Option<String>) -> Vec<String> {
    match verdict {
        CheckVerdict::Pass | CheckVerdict::Skip => vec![],
        _ => detail.iter().cloned().collect(),
    }
}

/// Build a human-readable headline from the first port's first IP result.
fn build_headline(ports: &[PortResult]) -> String {
    let first_ip = ports.first().and_then(|p| p.ips.first());

    let version = first_ip
        .and_then(|ip| ip.tls.as_ref())
        .map(|t| t.version.as_str())
        .unwrap_or("TLS");

    let expiry = first_ip
        .and_then(|ip| ip.chain.as_ref())
        .and_then(|chain| chain.first())
        .and_then(|cert| cert.days_remaining)
        .map(|days| format!(", expires in {days}d"));

    match expiry {
        Some(e) => format!("{version}{e}"),
        None => "TLS inspection complete".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Minimal percent-encoding for query string values
// ---------------------------------------------------------------------------

use super::percent_encode;
