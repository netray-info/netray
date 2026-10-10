use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::LazyLock;
use std::time::Instant;

use governor::{Quota, RateLimiter};
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::json;

use crate::config::{ConfigError, ModuleConfig};
use crate::error::AppError;
use crate::input::{self, Target};
use crate::quality::HealthCheck;
use crate::routes::{self, InspectRequest, InspectResponse, PortResult};
use crate::state::AppState;
use crate::tls::IpInspectionResult;
use crate::validate::CheckStatus;

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| {
    [
        "tls_reachable",
        "chain_trusted",
        "not_expired",
        "hostname_match",
        "chain_complete",
        "strong_signature",
        "key_strength",
        "expiry_window",
        "cert_lifetime",
        "san_quality",
        "aia_reachability",
        "tls_version",
        "forward_secrecy",
        "aead_cipher",
        "ct_logged",
        "ocsp_stapled",
        "caa_compliant",
        "dane_valid",
        "consistency",
        "alpn_consistency",
        "ech_advertised",
    ]
    .into_iter()
    .map(|name| CheckId::parse(&format!("tls.{name}")).expect("static check id"))
    .collect()
});

pub(crate) fn tls_checks() -> &'static [CheckId] {
    CHECKS.as_slice()
}

/// The TLS inspection as an engine module: tlsight's input parsing, the target policy and the
/// per-check handshake budget (cap-and-warn), its per-target limit (cost ports x inspected
/// addresses, charged before a blocked target is refused) and handshake semaphore, then the
/// inspection core.
pub struct TlsModule {
    state: AppState,
    per_target: KeyedLimiter<String>,
    check_budget: u32,
}

impl TlsModule {
    pub async fn new(config: ModuleConfig) -> Result<Self, ConfigError> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let check_budget = config.limits.check_budget;
        let config = config.into_config()?;
        let per_target = RateLimiter::keyed(
            Quota::per_minute(
                NonZeroU32::new(config.limits.per_target_per_minute).expect("validated non-zero"),
            )
            .allow_burst(
                NonZeroU32::new(config.limits.per_target_burst).expect("validated non-zero"),
            ),
        );
        let mut state = AppState::new(&config);
        state.init_dns_resolver(&config).await;
        Ok(Self {
            state,
            per_target,
            check_budget,
        })
    }

    async fn measure(&self, ctx: &RunContext) -> Result<InspectResponse, AppError> {
        let start = Instant::now();
        let config = self.state.config.load();
        let parsed = input::parse_input(ctx.domain.as_str(), config.limits.max_ports)?;
        let hostname = match &parsed.target {
            Target::Hostname(h) => h.clone(),
            Target::Ip(ip) => ip.to_string(),
        };

        let ips: Vec<IpAddr> = routes::resolve_target(&self.state, &parsed.target).await?;
        let selection = select_ips(
            ips,
            config.limits.allow_blocked_targets,
            &hostname,
            parsed.ports.len(),
            self.check_budget,
        );
        let inspected = selection.as_ref().map_or(1, |s| s.ips.len());
        let cost = parsed.ports.len() as u32 * inspected as u32;
        check_keyed_cost(
            &self.per_target,
            &hostname.to_lowercase(),
            NonZeroU32::new(cost.max(1)).expect("max(1) is non-zero"),
            "per_target",
            "tlsight",
        )
        .map_err(|r| AppError::RateLimited {
            retry_after_secs: r.retry_after_secs,
            scope: r.scope,
        })?;
        let Selection {
            ips,
            warnings,
            skipped_ips,
        } = selection?;

        routes::inspect(
            &self.state,
            &config,
            InspectRequest {
                parsed,
                ips,
                warnings,
                skipped_ips,
                request_id: String::new(),
                start,
            },
        )
        .await
    }
}

/// The addresses to inspect, the warnings so far and the addresses the budget skipped.
struct Selection {
    ips: Vec<IpAddr>,
    warnings: Vec<String>,
    skipped_ips: Vec<String>,
}

/// The target policy, then the addresses `budget` handshakes over `ports` ports cover.
fn select_ips(
    ips: Vec<IpAddr>,
    allow_blocked: bool,
    hostname: &str,
    ports: usize,
    budget: u32,
) -> Result<Selection, AppError> {
    let mut warnings = Vec::new();
    let allowed = routes::filter_allowed(ips, allow_blocked, hostname, &mut warnings)?;
    let (selected, skipped) = routes::cap_to_budget(&allowed, ports, budget, &mut warnings);
    Ok(Selection {
        ips: selected,
        warnings,
        skipped_ips: skipped,
    })
}

impl Module for TlsModule {
    fn protocol(&self) -> Protocol {
        Protocol::Tls
    }

    fn checks(&self) -> &'static [CheckId] {
        tls_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            match self.measure(ctx).await {
                Ok(resp) => translate(&resp),
                Err(e) => SectionOutcome::Incomplete {
                    reason: e.to_string(),
                },
            }
        })
    }
}

/// Maps the first port's quality checks onto `tls.<id>` checks and the V1 headline.
pub fn translate(resp: &InspectResponse) -> SectionOutcome {
    let mut checks: Vec<CheckResult> = Vec::new();

    // Collect per-port quality checks (cert, protocol, config) from the first port.
    // These are in ports[0].quality.checks (PortQualityResult).
    if let Some(port) = resp.ports.first()
        && let Some(port_quality) = port.quality.as_ref()
    {
        for hc in &port_quality.checks {
            let id = match CheckId::parse(&format!("tls.{}", hc.id)) {
                Ok(id) => id,
                Err(e) => {
                    return SectionOutcome::Incomplete {
                        reason: e.to_string(),
                    };
                }
            };
            checks.push(CheckResult {
                id,
                status: check_status(hc, &port.ips),
                findings: tls_check_messages(hc.status, &hc.detail),
                evidence: vec![],
            });
        }
    }

    let headline = build_headline(&resp.ports);

    SectionOutcome::Measured {
        checks,
        presentation: json!({ "headline": headline }),
    }
}

/// tlsight's status; a `tls_reachable` skip because this host cannot reach the target is
/// `NotTested`.
fn check_status(hc: &HealthCheck, ips: &[IpInspectionResult]) -> Status {
    if hc.id == "tls_reachable"
        && hc.status == CheckStatus::Skip
        && let Some(err) = ips.iter().find_map(|r| r.error.as_ref())
    {
        let status = crate::tls::status_of_error_code(&err.code);
        if status == Status::NotTested {
            return status;
        }
    }
    hc.status.into()
}

/// Return diagnostic messages for a TLS check — only for non-passing verdicts.
fn tls_check_messages(status: CheckStatus, detail: &str) -> Vec<String> {
    match status {
        CheckStatus::Pass | CheckStatus::Skip => vec![],
        _ => vec![detail.to_string()],
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
        .map(|cert| format!(", expires in {}d", cert.days_remaining));

    match expiry {
        Some(e) => format!("{version}{e}"),
        None => "TLS inspection complete".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public(n: u8) -> IpAddr {
        IpAddr::from([1, 1, 1, n])
    }

    #[test]
    fn over_budget_selects_representative_ips_with_warning() {
        let ips: Vec<IpAddr> = (1..=25).map(public).collect();
        let Selection {
            ips: selected,
            warnings,
            skipped_ips: skipped,
        } = select_ips(ips, false, "example.com", 1, 10).expect("allowed");
        assert_eq!(selected.len(), 10);
        assert_eq!(skipped.len(), 15);
        assert_eq!(
            warnings,
            ["rate limit: inspecting 10 of 25 IPs to stay within budget"]
        );
    }

    #[test]
    fn target_policy_applies_before_the_budget() {
        let blocked = (1..=6).map(|n| IpAddr::from([10, 0, 0, n]));
        let ips: Vec<IpAddr> = blocked.chain((1..=15).map(public)).collect();
        let Selection {
            ips: selected,
            warnings,
            skipped_ips: skipped,
        } = select_ips(ips, false, "example.com", 1, 10).expect("allowed");
        assert_eq!(selected, (1..=10).map(public).collect::<Vec<_>>());
        assert_eq!(skipped.len(), 5);
        assert_eq!(warnings.iter().filter(|w| w.contains("blocked")).count(), 6);
        assert_eq!(
            warnings.last().map(String::as_str),
            Some("rate limit: inspecting 10 of 15 IPs to stay within budget")
        );
    }
}
