use std::collections::HashMap;
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
use crate::dns::CaaLookup;
use crate::dns::https_record::ech_in_presentation;
use crate::error::AppError;
use crate::input::{self, Target};
use crate::quality::{self, HealthCheck, checks};
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

    async fn measure(&self, ctx: &RunContext, facts: &Facts) -> Result<InspectResponse, AppError> {
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
                caa: (!facts.caa.is_empty()).then(|| CaaLookup::from_presentation(&facts.caa)),
                ech: (!facts.https.is_empty()).then(|| ech_in_presentation(&facts.https)),
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

    fn needs_address_records(&self) -> bool {
        true
    }

    fn wants_facts(&self) -> bool {
        true
    }

    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            match self.measure(ctx, facts).await {
                Ok(resp) => translate(&resp),
                Err(e) => SectionOutcome::Incomplete {
                    reason: e.to_string(),
                },
            }
        })
    }
}

/// Maps port 443 (else the first port) onto all 21 `tls.<id>` checks and the V1 headline. Each
/// per-address check is measured on every address that completed a handshake and reports the
/// worst of them; CAA, DANE and OCSP come once from the summary. A failed CAA, TLSA or HTTPS
/// lookup makes its check `Unmeasured`, and so do the consistency checks when a timeout left
/// fewer than two handshakes. Without any handshake the other 20 checks follow
/// `tls_reachable`: not applicable behind a Fail, `NotTested` or `Unmeasured` otherwise.
pub fn translate(resp: &InspectResponse) -> SectionOutcome {
    let port = resp
        .ports
        .iter()
        .find(|p| p.port == 443)
        .or_else(|| resp.ports.first());
    let ips = port.map_or(&[][..], |p| p.ips.as_slice());
    let handshakes: Vec<&IpInspectionResult> = ips.iter().filter(|r| r.error.is_none()).collect();

    let mut by_id: HashMap<String, CheckResult> = HashMap::new();
    let reachable = reachable(&checks::check_tls_reachable(ips), ips);
    let reachable_status = reachable.status;
    by_id.insert("tls_reachable".to_string(), reachable);

    if let (Some(port), false) = (port, handshakes.is_empty()) {
        let is_hostname = resp.input_mode == "hostname";
        let ct_enabled = resp.summary.checks.ct_logged != CheckStatus::Skip;
        let per_address: Vec<(&str, Vec<HealthCheck>)> = handshakes
            .iter()
            .map(|r| {
                let mut checks =
                    quality::assess_address(r, port.port, is_hostname, &resp.hostname, ct_enabled);
                not_applicable_when_chain_complete(&mut checks);
                (r.ip.as_str(), checks)
            })
            .collect();
        for id in tls_checks() {
            let name = id.to_string();
            let name = name.trim_start_matches("tls.");
            let hcs: Vec<(&str, &HealthCheck)> = per_address
                .iter()
                .filter_map(|(ip, checks)| {
                    checks.iter().find(|hc| hc.id == name).map(|hc| (*ip, hc))
                })
                .collect();
            if let Some(r) = worst_of(name, &hcs) {
                by_id.insert(name.to_string(), r);
            }
        }

        let summary = &resp.summary.checks;
        let ocsp = summary.ocsp_stapled;
        for hc in [
            checks::check_ocsp_stapled(ocsp == CheckStatus::Pass, ocsp != CheckStatus::Skip),
            checks::check_caa_compliant(summary.caa_compliant),
            checks::check_dane_valid(summary.dane_valid),
            checks::check_consistency(port.consistency.as_ref()),
            checks::check_alpn_consistency(ips),
        ] {
            by_id.insert(
                hc.id.clone(),
                result(&hc.id, hc.status.into(), findings(&hc)),
            );
        }

        let lookup_errors = resp
            .dns
            .as_ref()
            .map_or(&[][..], |d| d.lookup_errors.as_slice());
        for (lookup, name, record) in [
            ("caa", "caa_compliant", "CAA"),
            ("tlsa", "dane_valid", "TLSA"),
            ("https", "ech_advertised", "HTTPS"),
        ] {
            if lookup_errors.iter().any(|e| e == lookup) && (lookup != "https" || port.port == 443)
            {
                let finding = format!("{record} lookup for {} failed", resp.hostname);
                by_id.insert(
                    name.to_string(),
                    result(name, Status::Unmeasured, vec![finding]),
                );
            }
        }

        let timed_out = timed_out(ips);
        if handshakes.len() < 2 && !timed_out.is_empty() {
            let finding = format!(
                "fewer than 2 handshakes to compare, timed out: {}",
                timed_out.join(", ")
            );
            for name in ["consistency", "alpn_consistency"] {
                by_id.insert(
                    name.to_string(),
                    result(name, Status::Unmeasured, vec![finding.clone()]),
                );
            }
        }
    }

    let (fallback_status, fallback) = if !handshakes.is_empty() {
        (Status::NotApplicable, "not reported")
    } else {
        match reachable_status {
            Status::NotTested => (Status::NotTested, "no TLS handshake"),
            Status::Unmeasured => (
                Status::Unmeasured,
                "no TLS handshake: every tested address timed out",
            ),
            _ => (Status::NotApplicable, "no TLS handshake"),
        }
    };
    let checks = tls_checks()
        .iter()
        .map(|id| {
            let name = id.to_string();
            let name = name.trim_start_matches("tls.");
            by_id
                .remove(name)
                .unwrap_or_else(|| result(name, fallback_status, vec![fallback.to_string()]))
        })
        .collect();

    SectionOutcome::Measured {
        checks,
        presentation: json!({ "headline": build_headline(&resp.ports) }),
        evidence: vec![],
    }
}

fn result(name: &str, status: Status, findings: Vec<String>) -> CheckResult {
    CheckResult {
        id: CheckId::parse(&format!("tls.{name}")).expect("static check id"),
        status,
        findings,
        evidence: vec![],
    }
}

/// The detail of a verdict that is not a pass.
fn findings(hc: &HealthCheck) -> Vec<String> {
    match hc.status {
        CheckStatus::Pass => vec![],
        _ => vec![hc.detail.clone()],
    }
}

/// An AIA pass because the chain is complete says nothing about AIA: not applicable.
fn not_applicable_when_chain_complete(checks: &mut [HealthCheck]) {
    let complete = checks
        .iter()
        .any(|hc| hc.id == "chain_complete" && hc.status != CheckStatus::Fail);
    if let Some(aia) = checks
        .iter_mut()
        .find(|hc| hc.id == "aia_reachability" && hc.status == CheckStatus::Pass)
        && complete
    {
        aia.status = CheckStatus::Skip;
    }
}

/// The worst verdict over the addresses (Fail > Warn > Pass > Skip); a Warn or Fail names each
/// address at that verdict, all-Skip is not applicable with the detail.
fn worst_of(name: &str, hcs: &[(&str, &HealthCheck)]) -> Option<CheckResult> {
    let rank = |s: CheckStatus| match s {
        CheckStatus::Skip => 0,
        CheckStatus::Pass => 1,
        CheckStatus::Warn => 2,
        CheckStatus::Fail => 3,
    };
    let worst = hcs
        .iter()
        .map(|(_, hc)| hc.status)
        .max_by_key(|s| rank(*s))?;
    let findings = match worst {
        CheckStatus::Pass => vec![],
        CheckStatus::Skip => vec![hcs[0].1.detail.clone()],
        _ => hcs
            .iter()
            .filter(|(_, hc)| hc.status == worst)
            .map(|(ip, hc)| format!("{ip}: {}", hc.detail))
            .collect(),
    };
    Some(result(name, worst.into(), findings))
}

/// `tls_reachable` from tlsight's verdict and the per-address error codes. A pass names each
/// address that timed out; otherwise a target-side failure on any tested address is `Fail`, a
/// timeout `Unmeasured`, and only untestable addresses `NotTested`.
fn reachable(hc: &HealthCheck, ips: &[IpInspectionResult]) -> CheckResult {
    if hc.status == CheckStatus::Pass {
        let findings = ips
            .iter()
            .filter_map(|r| Some((&r.ip, r.error.as_ref()?)))
            .filter(|(_, e)| e.code == "HANDSHAKE_TIMEOUT")
            .map(|(ip, e)| format!("{ip}: {}", e.message))
            .collect();
        return result(&hc.id, Status::Pass, findings);
    }
    let statuses: Vec<Status> = ips
        .iter()
        .filter_map(|r| r.error.as_ref())
        .map(|e| crate::tls::status_of_error_code(&e.code))
        .filter(|s| *s != Status::NotTested)
        .collect();
    if statuses.contains(&Status::Fail) {
        result(&hc.id, Status::Fail, vec![hc.detail.clone()])
    } else if statuses.contains(&Status::Unmeasured) {
        let unmeasured: Vec<String> = ips
            .iter()
            .filter_map(|r| Some((&r.ip, r.error.as_ref()?)))
            .filter(|(_, e)| crate::tls::status_of_error_code(&e.code) == Status::Unmeasured)
            .map(|(ip, e)| format!("{ip}: {}", e.message))
            .collect();
        result(&hc.id, Status::Unmeasured, unmeasured)
    } else {
        result(&hc.id, Status::NotTested, vec![hc.detail.clone()])
    }
}

/// The addresses whose handshake timed out.
fn timed_out(ips: &[IpInspectionResult]) -> Vec<&str> {
    ips.iter()
        .filter(|r| {
            r.error
                .as_ref()
                .is_some_and(|e| e.code == "HANDSHAKE_TIMEOUT")
        })
        .map(|r| r.ip.as_str())
        .collect()
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
