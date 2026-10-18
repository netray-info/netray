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

fn default_port() -> u16 {
    443
}

/// One host and port to inspect.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TlsInspectRequest {
    pub host: String,
    #[serde(default = "default_port")]
    #[schema(default = 443)]
    pub port: u16,
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
            | AppError::TooManyPorts { .. } => Self::InvalidInput(text),
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

    /// Inspects `req.host` on `req.port`; a port in `host` is ignored, as tlsight's POST does.
    pub async fn inspect(
        &self,
        req: &TlsInspectRequest,
        client: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<InspectResponse, TlsRefusal> {
        let max_ports = self.state.config.load().limits.max_ports;
        let mut parsed = input::parse_input(&req.host, max_ports)?;
        if req.port == 0 {
            return Err(AppError::InvalidPort("port must be 1-65535".to_string()).into());
        }
        parsed.ports = vec![req.port];
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
        let mut skipped_ips = Vec::new();

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

        // Cap-and-warn: compute cost and reduce IPs if over budget
        let full_cost = parsed.ports.len() as u32 * allowed_ips.len() as u32;
        let inspected_ips = if limits
            .check_cost(client_ip, &hostname_str, full_cost)
            .is_ok()
        {
            allowed_ips
        } else {
            // Try with reduced IPs
            let budget = limits.remaining_budget(client_ip);
            let (selected, skipped) =
                routes::cap_to_budget(&allowed_ips, parsed.ports.len(), budget, &mut warnings);

            if selected.is_empty() {
                // Even 1 IP doesn't fit — hard reject
                tracing::warn!(
                    client_ip = %client_ip,
                    scope = "per_ip",
                    "rate limited"
                );
                return Err(AppError::RateLimited {
                    retry_after_secs: 60,
                    scope: "per_ip",
                });
            }

            let reduced_cost = parsed.ports.len() as u32 * selected.len() as u32;
            if let Err(e) = limits.check_cost(client_ip, &hostname_str, reduced_cost) {
                if let AppError::RateLimited { scope, .. } = &e {
                    tracing::warn!(
                        client_ip = %client_ip,
                        scope = *scope,
                        "rate limited"
                    );
                }
                return Err(e);
            }

            skipped_ips = skipped;
            selected
        };

        routes::inspect(
            state,
            &config,
            InspectRequest {
                parsed,
                ips: inspected_ips,
                warnings,
                skipped_ips,
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
