//! The engine's protocol modules as lens sections: a `ModuleSection` runs the registry's module
//! in-process and maps its `SectionOutcome` onto lens's backend result.

use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use netray_engine::{Domain, Facts, Registry, RunContext, RunOptions, SectionOutcome};
use netray_model::{Protocol, Status};
use serde::Deserialize;

use crate::backends::{Backend, BackendContext, BackendExtra, BackendResult, percent_encode};
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
        "http"
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
            let module = self.registry.module(self.protocol).ok_or_else(|| {
                SectionError::BackendError(format!("no {:?} module registered", self.protocol))
            })?;
            let ctx = RunContext {
                deadline: Instant::now() + self.timeout,
                domain: Domain::new(domain.as_str()),
                options: RunOptions { dkim_selectors },
            };
            let outcome = tokio::time::timeout(self.timeout, module.run(&ctx, &Facts::default()))
                .await
                .map_err(|_| {
                    tracing::warn!(service = "http", url = %domain, error = "timeout", "backend call failed");
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
                    let p: HttpPresentation =
                        serde_json::from_value(presentation).unwrap_or_default();
                    Ok(BackendResult {
                        checks,
                        extra: BackendExtra::Http {
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
                        },
                    })
                }
                SectionOutcome::Incomplete { reason } => {
                    tracing::warn!(service = "http", url = %domain, error = %reason, "backend call failed");
                    Err(SectionError::BackendError(reason))
                }
                SectionOutcome::NotApplicable { reason } => {
                    Err(SectionError::NotApplicable { reason })
                }
            }
        })
    }
}
