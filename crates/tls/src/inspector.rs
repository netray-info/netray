//! The TLS inspection as a type: tlsight's input rules, target policy, rate-limit cost with
//! cap-and-warn and the inspection core, callable without an axum type.

use std::net::IpAddr;
use std::time::Instant;

use serde::Deserialize;
use utoipa::ToSchema;

use crate::error::AppError;
use crate::input::{self, ParsedInput, Target};
use crate::routes::{self, InspectRequest, InspectResponse};
use crate::security::RateLimitState;
use crate::state::AppState;

/// One host and its port or ports to inspect.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TlsInspectRequest {
    pub host: String,
    /// The port; 443 by default.
    #[serde(default)]
    #[schema(default = 443, value_type = u16)]
    pub port: Option<u16>,
    /// Several ports instead of `port`, at most `limits.max_ports`, each inspected once.
    #[serde(default)]
    #[schema(value_type = Vec<u16>)]
    pub ports: Option<Vec<u16>>,
}

fn dedup_ports(ports: impl IntoIterator<Item = u16>) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    for p in ports {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Why an inspection did not run to an answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TlsRefusal {
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Blocked(String),
    #[error("rate limit exceeded")]
    RateLimited { retry_after_secs: u64 },
    #[error("{0}")]
    Unavailable(String),
}

impl From<AppError> for TlsRefusal {
    fn from(e: AppError) -> Self {
        let text = e.to_string();
        match e {
            AppError::InvalidHostname(_)
            | AppError::InvalidPort(_)
            | AppError::ParseError(_)
            | AppError::AmbiguousInput(_)
            | AppError::TooManyPorts { .. }
            | AppError::CostExceedsLimit { .. } => Self::InvalidInput(text),
            AppError::BlockedTarget(_) => Self::Blocked(text),
            AppError::RateLimited {
                retry_after_secs, ..
            } => Self::RateLimited { retry_after_secs },
            AppError::DnsResolutionFailed(_)
            | AppError::ConnectionFailed(_)
            | AppError::HandshakeFailed(_)
            | AppError::CertificateError(_)
            | AppError::RequestTimeout => Self::Unavailable(text),
        }
    }
}

/// The inspection over one `AppState`: its config, trust store, handshake semaphore, resolver
/// and backend.
#[derive(Clone)]
pub struct TlsInspector {
    state: AppState,
}

impl TlsInspector {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// The connections the backend's probes of one address cost per port.
    pub fn probe_connections(&self) -> u32 {
        self.state.backend.probe_connections()
    }

    /// The most ports one inspection takes: `limits.max_ports`.
    pub fn max_ports(&self) -> usize {
        self.state.config.load().limits.max_ports
    }

    /// tlsight's input rules with this inspector's `limits.max_ports`.
    pub fn parse(&self, input: &str) -> Result<ParsedInput, AppError> {
        let mut parsed = input::parse_input(input, self.state.config.load().limits.max_ports)?;
        parsed.ports = dedup_ports(parsed.ports);
        Ok(parsed)
    }

    /// Inspects `req.host` on `req.port` or `req.ports` (443 when neither is given); a port in
    /// `host` is ignored, as tlsight's POST does.
    pub async fn inspect(
        &self,
        req: &TlsInspectRequest,
        client: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<InspectResponse, TlsRefusal> {
        let mut parsed = self.parse(&req.host)?;
        let ports = match (req.port, &req.ports) {
            (Some(_), Some(_)) => {
                return Err(
                    AppError::InvalidPort("give port or ports, not both".to_string()).into(),
                );
            }
            (None, Some(list)) if list.is_empty() => {
                return Err(AppError::InvalidPort("ports must not be empty".to_string()).into());
            }
            (None, Some(list)) => list.clone(),
            (Some(p), None) => vec![p],
            (None, None) => vec![443],
        };
        if ports.contains(&0) {
            return Err(AppError::InvalidPort("port must be 1-65535".to_string()).into());
        }
        let ports = dedup_ports(ports);
        let max = self.state.config.load().limits.max_ports;
        if ports.len() > max {
            return Err(AppError::TooManyPorts {
                requested: ports.len(),
                max,
            }
            .into());
        }
        parsed.ports = ports;
        Ok(self
            .inspect_parsed(parsed, client, limits, request_id)
            .await?)
    }

    /// Resolves, applies the target policy and the rate-limit cost (reducing the addresses to
    /// the budget with a warning), then inspects.
    pub async fn inspect_parsed(
        &self,
        parsed: ParsedInput,
        client_ip: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<InspectResponse, AppError> {
        let state = &self.state;
        let request_start = Instant::now();
        let config = state.config.load();

        let hostname_str = match &parsed.target {
            Target::Hostname(h) => h.clone(),
            Target::Ip(ip) => ip.to_string(),
        };
        let mut warnings = Vec::new();

        tracing::info!(
            hostname = %hostname_str,
            client_ip = %client_ip,
            request_id = %request_id,
            "inspect start"
        );

        let ips = routes::resolve_target(state, &parsed.target).await?;
        let allowed_ips = routes::filter_allowed(
            ips,
            config.limits.allow_blocked_targets,
            &hostname_str,
            &mut warnings,
        )
        .inspect_err(|_| {
            tracing::warn!(
                client_ip = %client_ip,
                target = %hostname_str,
                "blocked target: all resolved IPs in blocked ranges"
            );
        })?;

        // Cap-and-warn: the addresses and probes that fit the smaller burst, charged once. A
        // port count beyond a burst still plans one address and is refused as
        // `CostExceedsLimit` without a charge; probes are capped, never refused.
        let budget = limits.remaining_budget(client_ip);
        let plan = routes::plan_budget(
            &allowed_ips,
            parsed.ports.len(),
            budget,
            state.backend.probe_connections(),
            &mut warnings,
        );
        if let Err(e) = limits.check_cost(client_ip, &hostname_str, plan.cost) {
            if let AppError::RateLimited { scope, .. } = &e {
                tracing::warn!(
                    client_ip = %client_ip,
                    scope = *scope,
                    "rate limited"
                );
            }
            return Err(e);
        }

        routes::inspect(
            state,
            &config,
            InspectRequest {
                parsed,
                ips: plan.ips,
                probed: plan.probed,
                warnings,
                skipped_ips: plan.skipped,
                request_id,
                start: request_start,
                caa: None,
                ech: None,
            },
        )
        .await
    }
}

impl AppState {
    pub fn inspector(&self) -> TlsInspector {
        TlsInspector::new(self.clone())
    }
}
