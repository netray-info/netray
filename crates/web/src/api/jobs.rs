//! The inspector jobs of the `/inspect/*` pages over API v1's parts and limiters: one limiter
//! instance with `/api/v1`, one validation, one inspection per tool.
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use netray_engine::SectionOutcome;
use netray_model::evidence::Block;
use netray_model::{CheckResult, Protocol};
use netray_tls::input::Target;
use netray_tls::security::rate_limit::{Quotas, Scopes};

use super::error::V1Error;
use super::inspectors::{HTTP_OFF, TLS_OFF};
use super::ip::charge;
use super::state::ApiState;
use crate::pages::{InspectorJobs, Job, JobPart, JobRequest, Tool};

const NO_STARTTLS: &str = "STARTTLS for IMAP and FTP is not supported";
const IP_OFF: &str = "IP lookups are unavailable";
const DNS_SOON: &str = "DNS lookups are not available yet";

type Measured = Result<(Vec<Block>, Vec<CheckResult>), String>;

/// The inspector jobs over `ApiState`'s parts and `[api.*]` limiters.
pub struct InspectJobs {
    state: ApiState,
}

impl InspectJobs {
    pub fn new(state: ApiState) -> Self {
        Self { state }
    }
}

fn starttls_port(ports: &[u16]) -> bool {
    ports.iter().any(|port| matches!(port, 143 | 21))
}

fn measured(outcome: SectionOutcome) -> Measured {
    match outcome {
        SectionOutcome::Measured { checks, evidence } => Ok((evidence, checks)),
        SectionOutcome::NotApplicable { reason } | SectionOutcome::Incomplete { reason } => {
            Err(reason)
        }
        SectionOutcome::TimedOut => Err("timed out".into()),
    }
}

/// One inspection future, split into the part (evidence) and the band (checks).
fn job(label: &str, protocol: Protocol, run: BoxFuture<'static, Measured>) -> Job {
    let shared = run.shared();
    let evidence = shared.clone();
    let part = JobPart {
        label: label.into(),
        output: Box::pin(async move { evidence.await.map(|(blocks, _)| blocks) }),
    };
    Job::new(vec![part]).with_band(
        protocol,
        Box::pin(async move { shared.await.map(|(_, checks)| checks) }),
    )
}

fn retry_after(refusal: V1Error) -> Duration {
    match refusal {
        V1Error::RateLimited { retry_after_secs } => Duration::from_secs(retry_after_secs),
        _ => Duration::ZERO,
    }
}

impl InspectJobs {
    fn admit_tls(&self, client: IpAddr, input: &str) -> Result<(), Duration> {
        let Some(inspector) = self.state.tls_inspector() else {
            return Ok(());
        };
        let Ok(parsed) = inspector.parse(input) else {
            return Ok(());
        };
        if starttls_port(&parsed.ports) {
            return Ok(());
        }
        let target = match &parsed.target {
            Target::Hostname(host) => host.clone(),
            Target::Ip(ip) => ip.to_string(),
        };
        let limiters = self.state.tls_limiters();
        limiters.prune_if_due();
        limiters
            .check_cost(client, &target, parsed.ports.len().max(1) as u32)
            .map_err(|e| match e {
                netray_tls::error::AppError::RateLimited {
                    retry_after_secs, ..
                } => Duration::from_secs(retry_after_secs),
                _ => Duration::ZERO,
            })
    }

    fn admit_http(&self, client: IpAddr, input: &str) -> Result<(), Duration> {
        if self.state.http_inspect().is_none() {
            return Ok(());
        }
        let Ok(url) = netray_http::input::parse_url(input) else {
            return Ok(());
        };
        let limiters = self.state.http_limiters();
        limiters.prune_if_due();
        limiters
            .check(client, url.host_str().unwrap_or_default())
            .map_err(|e| match e {
                netray_http::error::AppError::RateLimited {
                    retry_after_secs, ..
                } => Duration::from_secs(retry_after_secs),
                _ => Duration::ZERO,
            })
    }

    fn admit_ip(&self, client: IpAddr, input: &str) -> Result<(), Duration> {
        if self.state.ip_lookup().is_none() {
            return Ok(());
        }
        let limiters = self.state.ip_limiters();
        limiters.prune_if_due();
        charge(&limiters.per_ip, client, 1, "api_v1_ip").map_err(retry_after)?;
        let input = input.trim();
        let target = if input.is_empty() {
            Some(client)
        } else {
            netray_ip::lookup::parse_address(input, self.state.config().ip.internal_mode).ok()
        };
        match target {
            Some(target) => {
                charge(&limiters.per_target, target, 1, "api_v1_ip_target").map_err(retry_after)
            }
            None => Ok(()),
        }
    }

    fn start_tls(&self, req: JobRequest) -> Result<Job, String> {
        let inspector = self
            .state
            .tls_inspector()
            .ok_or_else(|| TLS_OFF.to_string())?
            .clone();
        let parsed = inspector.parse(&req.input).map_err(|e| e.to_string())?;
        if starttls_port(&parsed.ports) {
            return Err(NO_STARTTLS.into());
        }
        let burst = self
            .state
            .config()
            .tls
            .per_ip_burst
            .max(parsed.ports.len() as u32)
            .max(1);
        let limiters = netray_tls::security::RateLimitState::scoped(
            &Quotas {
                per_ip_per_minute: burst,
                per_ip_burst: burst,
                per_target_per_minute: burst,
                per_target_burst: burst,
            },
            Scopes {
                metrics_prefix: "lens",
                per_ip: "inspect_tls",
                per_target: "inspect_tls_target",
            },
        );
        let client = req.client;
        Ok(job(
            "handshake",
            Protocol::Tls,
            Box::pin(async move {
                let resp = inspector
                    .inspect_parsed(parsed, client, &limiters, String::new())
                    .await
                    .map_err(|e| match e {
                        netray_tls::error::AppError::RateLimited {
                            retry_after_secs, ..
                        } => format!("too many requests; try again in {retry_after_secs} s"),
                        other => other.to_string(),
                    })?;
                measured(netray_tls::module::translate(&resp))
            }),
        ))
    }

    fn start_http(&self, req: JobRequest) -> Result<Job, String> {
        let backend = self
            .state
            .http_inspect()
            .ok_or_else(|| HTTP_OFF.to_string())?
            .clone();
        let url = netray_http::input::parse_url(&req.input).map_err(|e| e.to_string())?;
        Ok(job(
            "request",
            Protocol::Http,
            Box::pin(async move {
                let target = backend.target(&url).await.map_err(|e| e.to_string())?;
                let resp = backend
                    .inspect(&url, target, std::time::Instant::now())
                    .await
                    .map_err(|e| e.to_string())?;
                measured(netray_http::translate(&resp))
            }),
        ))
    }

    fn start_ip(&self, req: JobRequest) -> Result<Job, String> {
        let ip = self
            .state
            .ip_lookup()
            .ok_or_else(|| IP_OFF.to_string())?
            .clone();
        let input = req.input.trim();
        let addr = if input.is_empty() {
            req.client
        } else {
            netray_ip::lookup::parse_address(input, self.state.config().ip.internal_mode)
                .map_err(|e| e.to_string())?
        };
        let caller = input.is_empty();
        Ok(job(
            "lookup",
            Protocol::Ip,
            Box::pin(async move {
                let result = if caller {
                    ip.lookup_caller(SocketAddr::new(addr, 0), None).await
                } else {
                    ip.lookup(addr).await
                };
                measured(netray_ip::translate(
                    &[(addr, result.map_err(|e| e.to_string()))],
                    1,
                ))
            }),
        ))
    }
}

impl InspectorJobs for InspectJobs {
    fn admit(&self, tool: Tool, client: IpAddr, input: &str) -> Result<(), Duration> {
        match tool {
            Tool::Tls => self.admit_tls(client, input),
            Tool::Http => self.admit_http(client, input),
            Tool::Ip => self.admit_ip(client, input),
            Tool::Dns => Ok(()),
        }
    }

    fn start(&self, req: JobRequest) -> Result<Job, String> {
        match req.tool {
            Tool::Tls => self.start_tls(req),
            Tool::Http => self.start_http(req),
            Tool::Ip => self.start_ip(req),
            Tool::Dns => Err(DNS_SOON.into()),
        }
    }
}
