//! The engine's protocol modules as lens sections: a `ModuleSection` runs the registry's module
//! in-process and maps its `SectionOutcome` onto lens's backend result.

use std::collections::HashMap;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use netray_engine::{Domain, Registry, RunContext, RunOptions, RunReport, SectionOutcome};
use netray_model::{Protocol, Status};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::check::{HARD_DEADLINE, SectionError};
use crate::scoring::engine::{CheckResult, CheckVerdict};

/// Section-specific extra data produced by each backend.
#[derive(Clone, Debug)]
pub enum BackendExtra {
    Dns {
        resolved_ips: Vec<IpAddr>,
        raw_headline: String,
        detail_url: String,
    },
    Tls {
        raw_headline: String,
        detail_url: String,
    },
    Http {
        raw_headline: String,
        detail_url: String,
        status_code: Option<u16>,
        http_version: Option<String>,
        response_duration_ms: Option<u64>,
        server_ip: Option<String>,
        server_org: Option<String>,
        server_network_type: Option<String>,
    },
    Ip {
        addresses: Vec<IpInfo>,
        raw_headline: String,
        detail_url: String,
    },
    Email {
        raw_headline: String,
        detail_url: String,
        grade: Option<String>,
        /// Bucket name → reason for buckets that are not-applicable (e.g. no MX records).
        bucket_na: HashMap<String, String>,
    },
}

/// One enriched public address of the IP section.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct IpInfo {
    pub ip: IpAddr,
    pub org: Option<String>,
    pub geo: Option<String>,
    pub network_type: String,
}

#[derive(Clone, Debug)]
pub struct BackendResult {
    pub checks: Vec<CheckResult>,
    pub extra: BackendExtra,
}

/// Per-request context passed to every section.
#[derive(Clone)]
pub struct BackendContext {
    /// Validated DKIM selectors forwarded to the email backend.
    pub dkim_selectors: Option<Vec<String>>,
    /// Request headers for an HTTP backend; every section runs in-process, so none is read.
    pub forward_headers: reqwest::header::HeaderMap,
}

/// Minimal percent-encoding for query string values (RFC 3986 unreserved set).
pub(crate) fn percent_encode(s: &str) -> String {
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

pub trait Backend: Send + Sync {
    /// Must match the key in `ScoringProfile.sections`.
    fn section(&self) -> &'static str;

    /// Run the section on its own: one engine run with this section only.
    fn run(
        &self,
        domain: &str,
        context: &BackendContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BackendResult, SectionError>> + Send + '_>,
    >;
}

/// The V1 headline and extras a module carries in `presentation`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct HttpPresentation {
    headline: String,
    status_code: Option<u16>,
    http_version: Option<String>,
    response_duration_ms: Option<u64>,
    server_ip: Option<String>,
    server_org: Option<String>,
    server_network_type: Option<String>,
}

/// The V1 headline and extras the email module carries in `presentation`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct EmailPresentation {
    headline: String,
    grade: Option<String>,
    bucket_na: HashMap<String, String>,
}

/// The V1 headline and addresses the IP module carries in `presentation`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct IpPresentation {
    headline: String,
    addresses: Vec<IpInfo>,
}

/// The V1 headline and the resolved addresses the DNS module carries in `presentation`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct DnsPresentation {
    headline: String,
    resolved_ips: Vec<IpAddr>,
}

/// The V1 headline the TLS module carries in `presentation`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct TlsPresentation {
    headline: String,
}

fn section_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Dns => "dns",
        Protocol::Tls => "tls",
        Protocol::Http => "http",
        Protocol::Email => "email",
        Protocol::Ip => "ip",
    }
}

pub struct ModuleSection {
    pub registry: Arc<Registry>,
    pub protocol: Protocol,
    pub timeout: Duration,
    pub public_url: String,
}

fn verdict(status: Status) -> CheckVerdict {
    match status {
        Status::Pass => CheckVerdict::Pass,
        Status::Warn => CheckVerdict::Warn,
        Status::Fail => CheckVerdict::Fail,
        Status::NotApplicable | Status::NotTested | Status::Unmeasured => CheckVerdict::Skip,
    }
}

impl ModuleSection {
    /// Maps this section's outcome of an engine run onto lens's section result. The IP section
    /// of a run whose resolve stage answered no address is `NoDnsResults`.
    pub(crate) fn adapt(
        &self,
        domain: &str,
        outcome: SectionOutcome,
        report: &RunReport,
    ) -> Result<BackendResult, SectionError> {
        let section = section_name(self.protocol);
        if self.protocol == Protocol::Ip
            && report.resolve_error.is_none()
            && report.facts.a.is_empty()
            && report.facts.aaaa.is_empty()
        {
            return Err(SectionError::NoDnsResults);
        }
        match outcome {
            SectionOutcome::Measured {
                checks,
                presentation,
            } => {
                let checks = checks
                    .into_iter()
                    .map(|c| CheckResult {
                        name: c.id.name().to_string(),
                        verdict: verdict(c.status),
                        messages: c.findings,
                    })
                    .collect();
                let extra = match self.protocol {
                    Protocol::Email => {
                        let p: EmailPresentation =
                            serde_json::from_value(presentation).unwrap_or_default();
                        let base = if self.public_url.is_empty() {
                            "https://email.netray.info"
                        } else {
                            self.public_url.trim_end_matches('/')
                        };
                        BackendExtra::Email {
                            raw_headline: p.headline,
                            detail_url: format!("{base}/?domain={}", percent_encode(domain)),
                            grade: p.grade,
                            bucket_na: p.bucket_na,
                        }
                    }
                    Protocol::Ip => {
                        let p: IpPresentation =
                            serde_json::from_value(presentation).unwrap_or_default();
                        BackendExtra::Ip {
                            addresses: p.addresses,
                            raw_headline: p.headline,
                            detail_url: self.public_url.clone(),
                        }
                    }
                    Protocol::Dns => {
                        let p: DnsPresentation =
                            serde_json::from_value(presentation).unwrap_or_default();
                        BackendExtra::Dns {
                            resolved_ips: p.resolved_ips,
                            raw_headline: p.headline,
                            detail_url: format!(
                                "{}/?q={}+%2Bcheck",
                                self.public_url.trim_end_matches('/'),
                                percent_encode(domain),
                            ),
                        }
                    }
                    Protocol::Tls => {
                        let p: TlsPresentation =
                            serde_json::from_value(presentation).unwrap_or_default();
                        BackendExtra::Tls {
                            raw_headline: p.headline,
                            detail_url: format!(
                                "{}/?h={}",
                                self.public_url.trim_end_matches('/'),
                                percent_encode(domain),
                            ),
                        }
                    }
                    _ => {
                        let p: HttpPresentation =
                            serde_json::from_value(presentation).unwrap_or_default();
                        BackendExtra::Http {
                            raw_headline: p.headline,
                            detail_url: format!(
                                "{}/?url=https%3A%2F%2F{}",
                                self.public_url.trim_end_matches('/'),
                                percent_encode(domain),
                            ),
                            status_code: p.status_code,
                            http_version: p.http_version,
                            response_duration_ms: p.response_duration_ms,
                            server_ip: p.server_ip,
                            server_org: p.server_org,
                            server_network_type: p.server_network_type,
                        }
                    }
                };
                Ok(BackendResult { checks, extra })
            }
            SectionOutcome::Incomplete { reason } => {
                tracing::warn!(service = section, url = %domain, error = %reason, "backend call failed");
                Err(SectionError::BackendError(reason))
            }
            SectionOutcome::TimedOut => {
                tracing::warn!(service = section, url = %domain, error = "timeout", "backend call failed");
                Err(SectionError::Timeout)
            }
            SectionOutcome::NotApplicable { reason } => Err(SectionError::NotApplicable { reason }),
        }
    }
}

impl Backend for ModuleSection {
    fn section(&self) -> &'static str {
        section_name(self.protocol)
    }

    fn run(
        &self,
        domain: &str,
        context: &BackendContext,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<BackendResult, SectionError>> + Send + '_>>
    {
        let domain = domain.to_string();
        let dkim_selectors = context.dkim_selectors.clone();
        Box::pin(async move {
            if self.registry.module(self.protocol).is_none() {
                return Err(SectionError::BackendError(format!(
                    "no {:?} module registered",
                    self.protocol
                )));
            }
            let base = RunContext {
                deadline: Instant::now() + HARD_DEADLINE,
                domain: Domain::new(domain.as_str()),
                options: RunOptions { dkim_selectors },
            };
            let plan = [(self.protocol, self.timeout)];
            let (tx, mut rx) = mpsc::channel(1);
            let (report, event) = tokio::join!(
                netray_engine::run(&self.registry, base, &plan, HARD_DEADLINE, tx),
                rx.recv()
            );
            let outcome = event.map_or(SectionOutcome::TimedOut, |e| e.outcome);
            self.adapt(&domain, outcome, &report)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ip_section_returns_no_dns_results_when_empty() {
        let module = netray_ip::testing::golden_module(include_str!(
            "../../../tests/fixtures/contracts/ifconfig-json.json"
        ));
        let section = ModuleSection {
            registry: Arc::new(Registry::new().with(module)),
            protocol: Protocol::Ip,
            timeout: Duration::from_secs(5),
            public_url: "https://ip.example.com".to_string(),
        };
        let context = BackendContext {
            dkim_selectors: None,
            forward_headers: Default::default(),
        };
        let result = section.run("example.com", &context).await;
        assert!(
            matches!(result, Err(SectionError::NoDnsResults)),
            "expected NoDnsResults, got: {result:?}"
        );
    }
}
