//! The engine's protocol modules as lens sections: a `ModuleSection` runs the registry's module
//! in-process and maps its `SectionOutcome` onto lens's backend result.

use std::collections::HashMap;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use netray_engine::{Domain, Facts, Registry, RunContext, RunOptions, SectionOutcome};
use netray_model::{Protocol, Status};
use serde::Deserialize;

use crate::backends::{
    Backend, BackendContext, BackendExtra, BackendResult, IpInfo, percent_encode,
};
use crate::check::SectionError;
use crate::scoring::engine::{CheckResult, CheckVerdict};

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
        let resolved_ips = context.resolved_ips.clone();
        let section = self.section();
        Box::pin(async move {
            let mut facts = Facts::default();
            if self.protocol == Protocol::Ip {
                if resolved_ips.is_empty() {
                    return Err(SectionError::NoDnsResults);
                }
                for ip in resolved_ips {
                    match ip {
                        IpAddr::V4(a) => facts.a.push(a),
                        IpAddr::V6(aaaa) => facts.aaaa.push(aaaa),
                    }
                }
            }
            let module = self.registry.module(self.protocol).ok_or_else(|| {
                SectionError::BackendError(format!("no {:?} module registered", self.protocol))
            })?;
            let ctx = RunContext {
                deadline: Instant::now() + self.timeout,
                domain: Domain::new(domain.as_str()),
                options: RunOptions { dkim_selectors },
            };
            let outcome = tokio::time::timeout(self.timeout, module.run(&ctx, &facts))
                .await
                .map_err(|_| {
                    tracing::warn!(service = section, url = %domain, error = "timeout", "backend call failed");
                    SectionError::Timeout
                })?;
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
                                detail_url: format!("{base}/?domain={}", percent_encode(&domain)),
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
                                    percent_encode(&domain),
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
                                    percent_encode(&domain),
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
                                    percent_encode(&domain),
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
                SectionOutcome::NotApplicable { reason } => {
                    Err(SectionError::NotApplicable { reason })
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ip_section_returns_no_dns_results_when_empty() {
        let section = ModuleSection {
            registry: Arc::new(Registry::new()),
            protocol: Protocol::Ip,
            timeout: Duration::from_secs(5),
            public_url: "https://ip.example.com".to_string(),
        };
        let context = BackendContext {
            resolved_ips: vec![],
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
