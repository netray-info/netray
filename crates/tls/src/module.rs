use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use chrono::{DateTime, SecondsFormat, Utc};
use governor::{Quota, RateLimiter};
use netray_common::enrichment::EnrichmentClient;
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{
    Block, BlockId, Fields, Record as EvRecord, Register, Shape, ShapeKind, Value as EvValue,
    ValueType,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};

use crate::config::{ConfigError, ModuleConfig};
use crate::dns::AnsweredRecord;
use crate::dns::CaaLookup;
use crate::dns::https_record::ech_in_presentation;
use crate::error::AppError;
use crate::input::{self, Target};
use crate::quality::{self, HealthCheck, checks};
use crate::routes::{self, CaaSource, InspectRequest, InspectResponse, PortResult};
use crate::state::AppState;
use crate::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};
use crate::tls::{CertInfo, IpInspectionResult, TlsParams};
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
        "legacy_versions",
        "weak_suites",
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

/// The TLS register: the tested addresses, per address the served chain and the handshake, the
/// port's TLSA records, the SCTs per address and, without the facts' `dns.caa`, the CAA records.
pub const REGISTER: Register = Register {
    blocks: &[
        ("tls.addresses", ShapeKind::Table),
        ("tls.chain", ShapeKind::Chain),
        ("tls.handshake", ShapeKind::Fields),
        ("tls.probes", ShapeKind::Fields),
        ("tls.protocols", ShapeKind::Fields),
        ("tls.tlsa", ShapeKind::Records),
        ("tls.ct", ShapeKind::Table),
        ("tls.caa", ShapeKind::Records),
        ("tls.notices", ShapeKind::Fields),
    ],
    keys: &[
        ("tls.addresses", "address", ValueType::Addr),
        ("tls.addresses", "port", ValueType::Int),
        ("tls.addresses", "outcome", ValueType::Text),
        ("tls.addresses", "message", ValueType::Text),
        ("tls.chain", "subject", ValueType::Text),
        ("tls.chain", "issuer", ValueType::Text),
        ("tls.chain", "sans", ValueType::Text),
        ("tls.chain", "not_before", ValueType::Time),
        ("tls.chain", "not_after", ValueType::Time),
        ("tls.chain", "days_remaining", ValueType::Int),
        ("tls.chain", "key", ValueType::Text),
        ("tls.chain", "signature_algorithm", ValueType::Text),
        ("tls.chain", "fingerprint_sha256", ValueType::Text),
        ("tls.chain", "position", ValueType::Text),
        ("tls.chain", "ca_issuers_url", ValueType::Text),
        ("tls.chain", "serial", ValueType::Text),
        ("tls.chain", "fingerprint_sha1", ValueType::Text),
        ("tls.chain", "policy", ValueType::Text),
        ("tls.chain", "self_signed", ValueType::Bool),
        ("tls.chain", "expired", ValueType::Bool),
        ("tls.chain", "ocsp_url", ValueType::Text),
        ("tls.handshake", "version", ValueType::Text),
        ("tls.handshake", "cipher_suite", ValueType::Text),
        ("tls.handshake", "alpn", ValueType::Text),
        ("tls.handshake", "key_exchange_group", ValueType::Text),
        ("tls.handshake", "ocsp_stapled", ValueType::Bool),
        ("tls.handshake", "ech", ValueType::Bool),
        ("tls.handshake", "handshake_ms", ValueType::Duration),
        ("tls.handshake", "sni", ValueType::Text),
        ("tls.handshake", "starttls", ValueType::Text),
        ("tls.handshake", "ocsp_status", ValueType::Text),
        ("tls.handshake", "ocsp_this_update", ValueType::Time),
        ("tls.handshake", "ocsp_next_update", ValueType::Time),
        ("tls.handshake", "ocsp_live_status", ValueType::Text),
        ("tls.handshake", "ocsp_live_revoked_at", ValueType::Time),
        ("tls.handshake", "ocsp_live_reason", ValueType::Text),
        ("tls.probes", "ssl3", ValueType::Text),
        ("tls.probes", "tls1_0", ValueType::Text),
        ("tls.probes", "tls1_1", ValueType::Text),
        ("tls.probes", "tls1_2", ValueType::Text),
        ("tls.probes", "tls1_3", ValueType::Text),
        ("tls.probes", "suites_insecure", ValueType::Text),
        ("tls.probes", "suites_cbc", ValueType::Text),
        ("tls.probes", "suites_static_rsa", ValueType::Text),
        ("tls.probes", "ssl3_answer", ValueType::Text),
        ("tls.probes", "tls1_0_answer", ValueType::Text),
        ("tls.probes", "tls1_1_answer", ValueType::Text),
        ("tls.probes", "tls1_2_answer", ValueType::Text),
        ("tls.probes", "tls1_3_answer", ValueType::Text),
        ("tls.probes", "suites_insecure_answer", ValueType::Text),
        ("tls.probes", "suites_cbc_answer", ValueType::Text),
        ("tls.probes", "suites_static_rsa_answer", ValueType::Text),
        ("tls.protocols", "ssl3", ValueType::Text),
        ("tls.protocols", "tls1_0", ValueType::Text),
        ("tls.protocols", "tls1_1", ValueType::Text),
        ("tls.protocols", "tls1_2", ValueType::Text),
        ("tls.protocols", "tls1_3", ValueType::Text),
        ("tls.protocols", "suites_insecure", ValueType::Text),
        ("tls.protocols", "suites_cbc", ValueType::Text),
        ("tls.protocols", "suites_static_rsa", ValueType::Text),
        (
            "tls.protocols",
            "forward_secrecy_all_suites",
            ValueType::Bool,
        ),
        ("tls.tlsa", "owner", ValueType::Text),
        ("tls.tlsa", "type", ValueType::Text),
        ("tls.tlsa", "ttl", ValueType::Int),
        ("tls.tlsa", "data", ValueType::Text),
        ("tls.ct", "address", ValueType::Addr),
        ("tls.ct", "sct_count", ValueType::Int),
        ("tls.ct", "log_ids", ValueType::Text),
        ("tls.caa", "owner", ValueType::Text),
        ("tls.caa", "type", ValueType::Text),
        ("tls.caa", "ttl", ValueType::Int),
        ("tls.caa", "data", ValueType::Text),
        ("tls.notices", "warning", ValueType::Text),
        ("tls.notices", "skipped_address", ValueType::Addr),
    ],
};

const VOLATILE: &[EvidencePath] = &[
    EvidencePath("tls.chain/days_remaining"),
    EvidencePath("tls.handshake/handshake_ms"),
    EvidencePath("tls.handshake/ocsp_this_update"),
    EvidencePath("tls.handshake/ocsp_next_update"),
    EvidencePath("tls.handshake/ocsp_live_status"),
    EvidencePath("tls.handshake/ocsp_live_revoked_at"),
    EvidencePath("tls.handshake/ocsp_live_reason"),
    EvidencePath("tls.tlsa/ttl"),
    EvidencePath("tls.caa/ttl"),
];

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
    pub async fn new(
        config: ModuleConfig,
        enrichment: Option<Arc<EnrichmentClient>>,
    ) -> Result<Self, ConfigError> {
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
        state.enrichment_client = enrichment;
        state.init_dns_resolver(&config).await;
        Ok(Self {
            state,
            per_target,
            check_budget,
        })
    }

    /// The inspection over this module's own state.
    pub fn inspector(&self) -> crate::inspector::TlsInspector {
        self.state.inspector()
    }

    /// Whether the registry handed this module an enrichment client.
    pub fn has_enrichment(&self) -> bool {
        self.state.enrichment_client.is_some()
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
        // Probe connections count: the plan fits handshakes and probes into the check budget and
        // the per-target burst it is charged to.
        let selection = select_ips(
            ips,
            config.limits.allow_blocked_targets,
            &hostname,
            parsed.ports.len(),
            self.check_budget.min(config.limits.per_target_burst),
            self.state.backend.probe_connections(),
        );
        let cost = selection
            .as_ref()
            .map_or(parsed.ports.len() as u32, |s| s.plan.cost);
        check_keyed_cost(
            &self.per_target,
            &hostname.to_lowercase(),
            NonZeroU32::new(cost.max(1)).expect("max(1) is non-zero"),
            "per_target",
            "netray_tls",
        )
        .map_err(|r| AppError::RateLimited {
            retry_after_secs: r.retry_after_secs,
            scope: r.scope,
        })?;
        let Selection { plan, warnings } = selection?;

        routes::inspect(
            &self.state,
            &config,
            InspectRequest {
                parsed,
                ips: plan.ips,
                probed: plan.probed,
                warnings,
                skipped_ips: plan.skipped,
                request_id: String::new(),
                start,
                caa: (!facts.caa.is_empty()).then(|| CaaLookup::from_presentation(&facts.caa)),
                ech: (!facts.https.is_empty()).then(|| ech_in_presentation(&facts.https)),
            },
        )
        .await
    }
}

/// The plan (addresses, probed addresses, cost) and the warnings so far.
struct Selection {
    plan: routes::Plan,
    warnings: Vec<String>,
}

/// The target policy, then the plan for `budget` over `ports` ports with `probes` connections
/// per probed address.
fn select_ips(
    ips: Vec<IpAddr>,
    allow_blocked: bool,
    hostname: &str,
    ports: usize,
    budget: u32,
    probes: u32,
) -> Result<Selection, AppError> {
    let mut warnings = Vec::new();
    let allowed = routes::filter_allowed(ips, allow_blocked, hostname, &mut warnings)?;
    let plan = routes::plan_budget(&allowed, ports, budget, probes, &mut warnings);
    Ok(Selection { plan, warnings })
}

impl Module for TlsModule {
    fn protocol(&self) -> Protocol {
        Protocol::Tls
    }

    fn checks(&self) -> &'static [CheckId] {
        tls_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        VOLATILE
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
                Ok(resp) => {
                    let facts: Vec<BlockId> = facts.blocks.iter().map(|b| b.id.clone()).collect();
                    translate_with_facts(&resp, &facts)
                }
                Err(e) => SectionOutcome::Incomplete {
                    reason: e.to_string(),
                },
            }
        })
    }
}

/// Maps port 443 (else the first port) onto all 23 `tls.<id>` checks and its blocks. Each
/// per-address check is measured on every address that completed a handshake and reports the
/// worst of them; CAA, DANE and OCSP come once from the summary. A failed CAA, TLSA or HTTPS
/// lookup makes its check `Unmeasured`, and so do the consistency checks when a timeout left
/// fewer than two handshakes. Without any handshake the other 22 checks follow
/// `tls_reachable`: not applicable behind a Fail, `NotTested` or `Unmeasured` otherwise.
pub fn translate(resp: &InspectResponse) -> SectionOutcome {
    translate_with_facts(resp, &[])
}

/// [`translate`] with the ids of the facts blocks: `caa_compliant` references the facts'
/// `dns.caa` when it was judged from them, else TLS's own `tls.caa` or the chains, and
/// `ech_advertised` references the facts' `dns.https`.
pub fn translate_with_facts(resp: &InspectResponse, facts: &[BlockId]) -> SectionOutcome {
    translate_with_facts_at(resp, facts, Utc::now())
}

/// [`translate_with_facts`] judging a certificate's expiry against `now`.
pub(crate) fn translate_with_facts_at(
    resp: &InspectResponse,
    facts: &[BlockId],
    now: DateTime<Utc>,
) -> SectionOutcome {
    let port = resp
        .ports
        .iter()
        .find(|p| p.port == 443)
        .or_else(|| resp.ports.first());
    translate_port(resp, port, facts, false, true, now)
}

/// One outcome per inspected port, in the response's port order: each port is judged like
/// [`translate`] judges its port, with `tls.addresses@<port>` and `tls.ct@<port>` when there are
/// several ports, and OCSP, CAA and DANE judged on the port itself. The host-wide `tls.caa`
/// block and a `tls.notices` block of the run's warnings and parseable skipped addresses sit in
/// the [`band_port`]'s outcome only.
/// A response with one port gives [`translate`]'s outcome plus `tls.notices` when present; one
/// without a port gives nothing.
pub fn translate_ports(resp: &InspectResponse) -> Vec<(u16, SectionOutcome)> {
    let mut outcomes: Vec<(u16, SectionOutcome)> = match resp.ports.as_slice() {
        [] => Vec::new(),
        [only] => vec![(only.port, translate(resp))],
        ports => {
            let band = band_port(ports.iter().map(|p| p.port));
            ports
                .iter()
                .map(|p| {
                    (
                        p.port,
                        translate_port(resp, Some(p), &[], true, p.port == band, Utc::now()),
                    )
                })
                .collect()
        }
    };
    let mut fields: Fields = resp
        .warnings
        .iter()
        .map(|w| ("warning".to_string(), EvValue::Text(w.clone())))
        .collect();
    fields.extend(
        resp.skipped_ips
            .iter()
            .filter_map(|ip| ip.parse::<IpAddr>().ok())
            .map(|ip| ("skipped_address".to_string(), EvValue::Addr(ip))),
    );
    let band = band_port(outcomes.iter().map(|(p, _)| *p));
    if let (false, Some((_, SectionOutcome::Measured { evidence, .. }))) = (
        fields.is_empty(),
        outcomes.iter_mut().find(|(p, _)| *p == band),
    ) {
        evidence.push(Block {
            id: static_block_id("tls.notices"),
            title: "Notices".into(),
            shape: Shape::Fields(fields),
        });
    }
    outcomes
}

/// The port whose checks make an inspection's band and whose outcome holds the host-wide blocks:
/// 443 when inspected, else the lowest port; 443 for none.
pub fn band_port(ports: impl IntoIterator<Item = u16>) -> u16 {
    let ports: Vec<u16> = ports.into_iter().collect();
    if ports.contains(&443) {
        443
    } else {
        ports.into_iter().min().unwrap_or(443)
    }
}

fn translate_port(
    resp: &InspectResponse,
    port: Option<&PortResult>,
    facts: &[BlockId],
    qualify: bool,
    host_wide: bool,
    now: DateTime<Utc>,
) -> SectionOutcome {
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
            if let Some(r) = worst_of(name, &hcs, port.port) {
                by_id.insert(name.to_string(), r);
            }
        }
        let probed: Vec<(&IpInspectionResult, &ProtocolProbes)> = handshakes
            .iter()
            .filter_map(|r| Some((*r, r.probes.as_ref()?)))
            .collect();
        if !probed.is_empty() {
            let protocols = qualified_block_id("tls.protocols", port.port, qualify);
            for (name, judge) in [
                (
                    "legacy_versions",
                    legacy_versions as fn(&str, &ProtocolProbes) -> (Status, Vec<String>),
                ),
                ("weak_suites", weak_suites),
            ] {
                let per: Vec<(&str, Status, Vec<String>)> = probed
                    .iter()
                    .map(|(r, p)| {
                        let (status, findings) = judge(&r.ip, p);
                        (r.ip.as_str(), status, findings)
                    })
                    .collect();
                by_id.insert(
                    name.to_string(),
                    probe_check(name, &per, port.port, &protocols),
                );
            }
        }

        let summary = &resp.summary.checks;
        let ocsp = summary.ocsp_stapled;
        let (ocsp_check, caa, dane) = if qualify {
            port_wide(resp, port, &handshakes)
        } else {
            (
                checks::check_ocsp_stapled(ocsp == CheckStatus::Pass, ocsp != CheckStatus::Skip),
                summary.caa_compliant,
                summary.dane_valid,
            )
        };
        for hc in [
            ocsp_check,
            checks::check_caa_compliant(caa),
            checks::check_dane_valid(dane),
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
            // With several ports a TLSA error names no port: a port whose records answered is
            // measured, any other is not.
            let answered_here = qualify && lookup == "tlsa" && port.tlsa.is_some();
            if lookup_errors.iter().any(|e| e == lookup)
                && (lookup != "https" || port.port == 443)
                && !answered_here
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
    let caa_from_facts = caa_judged_from_facts(resp, facts);
    let evidence = port.map_or_else(Vec::new, |port| {
        blocks(resp, port, caa_from_facts, qualify, host_wide, now)
    });
    let checks = tls_checks()
        .iter()
        .map(|id| {
            let name = id.to_string();
            let name = name.trim_start_matches("tls.");
            let mut r = by_id
                .remove(name)
                .unwrap_or_else(|| result(name, fallback_status, vec![fallback.to_string()]));
            if let Some(refs) = shared_references(name, &evidence, caa_from_facts) {
                r.evidence = refs;
            }
            // Rule M: a check whose answer was not obtained points at no block.
            if r.status == Status::Unmeasured {
                r.evidence.clear();
            }
            r.evidence
                .retain(|id| evidence.iter().any(|b| b.id == *id) || facts.contains(id));
            r
        })
        .collect();

    SectionOutcome::Measured { checks, evidence }
}

fn status_rank(status: Status) -> u8 {
    match status {
        Status::Fail => 3,
        Status::Unmeasured => 2,
        Status::Warn => 1,
        _ => 0,
    }
}

/// The check over the probed addresses: the worst address status (fail, unmeasured, warn, pass)
/// with the findings and `tls.probes` blocks of the addresses at it, plus `tls.protocols`; an
/// unmeasured check references nothing.
fn probe_check(
    name: &str,
    per: &[(&str, Status, Vec<String>)],
    port: u16,
    protocols: &BlockId,
) -> CheckResult {
    let status = per
        .iter()
        .map(|(_, s, _)| *s)
        .max_by_key(|s| status_rank(*s))
        .unwrap_or(Status::Pass);
    let at_status = || per.iter().filter(|(_, s, _)| *s == status);
    let mut r = result(
        name,
        status,
        at_status().flat_map(|(_, _, f)| f.clone()).collect(),
    );
    if status != Status::Unmeasured {
        r.evidence = at_status()
            .filter_map(|(ip, _, _)| address_block_id("probes", ip, port))
            .chain(std::iter::once(protocols.clone()))
            .collect();
    }
    r
}

fn unanswered_findings(ip: &str, probes: &[(&str, &ProbeOutcome)]) -> Vec<String> {
    probes
        .iter()
        .filter(|(_, o)| o.verdict == Verdict::Unanswered)
        .map(|(name, o)| format!("{ip}: no answer to the {name} probe ({})", o.detail))
        .collect()
}

/// SSLv3, TLS 1.0 and TLS 1.1 on one address: any accepted is a fail, all refused a pass.
fn legacy_versions(ip: &str, probes: &ProtocolProbes) -> (Status, Vec<String>) {
    let versions = [
        ("SSLv3", &probes.ssl3),
        ("TLS 1.0", &probes.tls1_0),
        ("TLS 1.1", &probes.tls1_1),
    ];
    let accepted: Vec<&str> = versions
        .iter()
        .filter(|(_, o)| o.verdict == Verdict::Accepted)
        .map(|(n, _)| *n)
        .collect();
    if !accepted.is_empty() {
        return (
            Status::Fail,
            vec![format!("{ip}: accepts {}", accepted.join(", "))],
        );
    }
    if versions.iter().all(|(_, o)| o.verdict == Verdict::Refused) {
        return (Status::Pass, vec![]);
    }
    (Status::Unmeasured, unanswered_findings(ip, &versions))
}

/// The weak suite groups on one address: an accepted insecure group is a fail, an accepted CBC or
/// static RSA group a warn.
fn weak_suites(ip: &str, probes: &ProtocolProbes) -> (Status, Vec<String>) {
    let suite = |o: &ProbeOutcome| o.cipher_suite.clone().unwrap_or_else(|| "unknown".into());
    if probes.suites_insecure.verdict == Verdict::Accepted {
        let finding = format!(
            "{ip}: accepts insecure suite {}",
            suite(&probes.suites_insecure)
        );
        return (Status::Fail, vec![finding]);
    }
    let groups = [
        ("insecure suites", &probes.suites_insecure),
        ("CBC suites", &probes.suites_cbc),
        ("static RSA suites", &probes.suites_static_rsa),
    ];
    if probes.suites_insecure.verdict == Verdict::Unanswered {
        return (Status::Unmeasured, unanswered_findings(ip, &groups));
    }
    let warns: Vec<String> = [
        ("CBC", &probes.suites_cbc),
        ("static RSA", &probes.suites_static_rsa),
    ]
    .iter()
    .filter(|(_, o)| o.verdict == Verdict::Accepted)
    .map(|(n, o)| format!("{ip}: accepts {n} suite {}", suite(o)))
    .collect();
    if !warns.is_empty() {
        return (Status::Warn, warns);
    }
    if groups.iter().all(|(_, o)| o.verdict == Verdict::Refused) {
        return (Status::Pass, vec![]);
    }
    (Status::Unmeasured, unanswered_findings(ip, &groups))
}

/// The OCSP, CAA and DANE verdicts of one port of several, which the response's summary judges
/// from the first handshake and leaf over all ports: OCSP from the port's first handshake, CAA
/// from the port's leaf against the looked-up records (the summary's verdict when the records
/// are not at hand), DANE from the port's own TLSA match.
fn port_wide(
    resp: &InspectResponse,
    port: &PortResult,
    handshakes: &[&IpInspectionResult],
) -> (HealthCheck, CheckStatus, CheckStatus) {
    let first = handshakes.first();
    let leaf = first.and_then(|r| r.chain.as_ref()?.first());
    let ocsp = checks::check_ocsp_stapled(
        first.is_some_and(|r| r.tls.as_ref().is_some_and(|t| t.ocsp.stapled)),
        leaf.is_some_and(|l| l.ocsp_url.is_some()),
    );
    let lookup = resp
        .dns
        .as_ref()
        .and_then(|d| d.caa.as_ref())
        .and_then(|c| c.lookup.as_ref());
    let caa = match (lookup, leaf) {
        (Some(lookup), Some(leaf)) => {
            crate::validate::caa_compliance::check_caa_compliance(lookup, &leaf.issuer, &leaf.sans)
        }
        _ => resp.summary.checks.caa_compliant,
    };
    let dane = match port.tlsa.as_ref().and_then(|t| t.dane_valid) {
        Some(true) => CheckStatus::Pass,
        Some(false) => CheckStatus::Fail,
        None => CheckStatus::Skip,
    };
    (ocsp, caa, dane)
}

/// The id `tls.<name>@<addr>:<port>`, `None` for an address or port no block id can hold.
fn address_block_id(name: &str, ip: &str, port: u16) -> Option<BlockId> {
    let addr = SocketAddr::new(ip.parse().ok()?, port);
    BlockId::parse(&format!("tls.{name}@{addr}")).ok()
}

fn qualified_block_id(id: &str, port: u16, qualify: bool) -> BlockId {
    if qualify {
        BlockId::parse(&format!("{id}@{port}")).expect("static block id with a port")
    } else {
        static_block_id(id)
    }
}

fn static_block_id(id: &str) -> BlockId {
    BlockId::parse(id).expect("static block id")
}

/// Whether the CAA verdict was judged from the facts' `dns.caa`: TLS used the facts' records, or,
/// read from JSON, it holds no records of its own while the facts carry `dns.caa`.
fn caa_judged_from_facts(resp: &InspectResponse, facts: &[BlockId]) -> bool {
    let Some(caa) = resp.dns.as_ref().and_then(|d| d.caa.as_ref()) else {
        return false;
    };
    match caa.answered {
        Some(CaaSource::Facts) => true,
        Some(CaaSource::Lookup(_)) => false,
        None => caa.records.is_empty() && facts.contains(&static_block_id("dns.caa")),
    }
}

/// The references of a check judged from the whole port rather than per address; `None` for the
/// per-address checks, which keep the blocks of their deciding addresses. Without their record
/// block, `caa_compliant` and `dane_valid` reference the chains they matched.
fn shared_references(name: &str, blocks: &[Block], caa_from_facts: bool) -> Option<Vec<BlockId>> {
    let named = |block_name: &str| -> Vec<BlockId> {
        blocks
            .iter()
            .filter(|b| b.id.module() == "tls" && b.id.name() == block_name)
            .map(|b| b.id.clone())
            .collect()
    };
    let refs = match name {
        "tls_reachable" => named("addresses"),
        "ocsp_stapled" | "alpn_consistency" => named("handshake"),
        "consistency" => {
            let mut refs = named("chain");
            refs.extend(named("handshake"));
            refs
        }
        "ech_advertised" => {
            let mut refs = named("handshake");
            refs.push(static_block_id("dns.https"));
            refs
        }
        "aia_reachability" => named("chain"),
        "caa_compliant" => {
            if caa_from_facts {
                vec![static_block_id("dns.caa")]
            } else {
                Some(named("caa"))
                    .filter(|refs| !refs.is_empty())
                    .unwrap_or_else(|| named("chain"))
            }
        }
        "dane_valid" => Some(named("tlsa"))
            .filter(|refs| !refs.is_empty())
            .unwrap_or_else(|| named("chain")),
        "ct_logged" => named("ct"),
        _ => return None,
    };
    Some(refs)
}

/// The blocks of `port`: its tested addresses, the chain and handshake of each address that
/// completed one, the SCTs when CT is checked, and the TLSA and, unless CAA was judged from the
/// facts, CAA records as answered. A response read from JSON holds no answered records: it gets
/// no TLSA block, and a CAA block only for an empty answer, which needs no TTL.
fn blocks(
    resp: &InspectResponse,
    port: &PortResult,
    caa_from_facts: bool,
    qualify: bool,
    host_wide: bool,
    now: DateTime<Utc>,
) -> Vec<Block> {
    let mut blocks = vec![addresses_block(port, qualify)];
    let dns = resp.dns.as_ref();
    let failed = |lookup: &str| dns.is_some_and(|d| d.lookup_errors.iter().any(|e| e == lookup));
    let handshakes: Vec<(IpAddr, &IpInspectionResult)> = port
        .ips
        .iter()
        .filter(|r| r.error.is_none())
        .filter_map(|r| Some((r.ip.parse().ok()?, r)))
        .collect();
    for (ip, r) in &handshakes {
        let qualifier = SocketAddr::new(*ip, port.port);
        if let (Some(chain), Some(id)) = (&r.chain, address_block_id("chain", &r.ip, port.port)) {
            blocks.push(Block {
                id,
                title: format!("Certificate chain {qualifier}"),
                shape: Shape::Chain(chain.iter().map(|c| cert_fields(c, now)).collect()),
            });
        }
        if let (Some(tls), Some(id)) = (&r.tls, address_block_id("handshake", &r.ip, port.port)) {
            blocks.push(Block {
                id,
                title: format!("Handshake {qualifier}"),
                shape: Shape::Fields(handshake_fields(tls, failed("https"))),
            });
        }
    }
    blocks.extend(probe_blocks(port, &handshakes, qualify));
    if resp.summary.checks.ct_logged != CheckStatus::Skip && !handshakes.is_empty() {
        blocks.push(ct_block(&handshakes, qualify.then_some(port.port)));
    }
    let tlsa = port.tlsa.as_ref().and_then(|t| t.answered.as_deref());
    if let (Some(answered), Ok(id)) = (tlsa, BlockId::parse(&format!("tls.tlsa@{}", port.port))) {
        blocks.push(Block {
            id,
            title: "TLSA".into(),
            shape: Shape::Records(records("TLSA", answered)),
        });
    }
    let caa = dns
        .and_then(|d| d.caa.as_ref())
        .and_then(|caa| match &caa.answered {
            Some(CaaSource::Lookup(answered)) => Some(records("CAA", answered)),
            Some(CaaSource::Facts) => None,
            None => (caa.records.is_empty() && !failed("caa")).then(Vec::new),
        });
    if let (Some(caa), false, true) = (caa, caa_from_facts, host_wide) {
        blocks.push(Block {
            id: static_block_id("tls.caa"),
            title: "CAA".into(),
            shape: Shape::Records(caa),
        });
    }
    blocks
}

/// `tls.probes` per probed address, then the port's `tls.protocols`: a probe the server left
/// unanswered is omitted, addresses that differ read `mixed`.
fn probe_blocks(
    port: &PortResult,
    handshakes: &[(IpAddr, &IpInspectionResult)],
    qualify: bool,
) -> Vec<Block> {
    let probed: Vec<(&IpInspectionResult, &ProtocolProbes)> = handshakes
        .iter()
        .filter_map(|(_, r)| Some((*r, r.probes.as_ref()?)))
        .collect();
    let text = |k: &str, v: &str| (k.to_string(), EvValue::Text(v.to_string()));
    let word = |key: &str, verdict: Verdict| match (key.starts_with("suites_"), verdict) {
        (false, Verdict::Accepted) => Some("enabled"),
        (false, Verdict::Refused) => Some("disabled"),
        (true, Verdict::Accepted) => Some("accepted"),
        (true, Verdict::Refused) => Some("refused"),
        (_, Verdict::Unanswered) => None,
    };
    let mut blocks = Vec::new();
    for (r, probes) in &probed {
        let Some(id) = address_block_id("probes", &r.ip, port.port) else {
            continue;
        };
        let mut fields = Fields::new();
        for (key, outcome) in probes.entries() {
            if let Some(value) = word(key, outcome.verdict) {
                fields.push(text(key, value));
                fields.push(text(&format!("{key}_answer"), &outcome.detail));
            }
        }
        let addr = SocketAddr::new(r.ip.parse().expect("block id parsed it"), port.port);
        blocks.push(Block {
            id,
            title: format!("Protocol probes {addr}"),
            shape: Shape::Fields(fields),
        });
    }
    if probed.is_empty() {
        return blocks;
    }
    let mut fields = Fields::new();
    for (i, (key, _)) in probed[0].1.entries().into_iter().enumerate() {
        let values: Vec<Option<&str>> = probed
            .iter()
            .map(|(_, p)| word(key, p.entries()[i].1.verdict))
            .collect();
        if values.contains(&None) {
            continue;
        }
        let first = values[0].expect("no unanswered value");
        let all_equal = values.iter().all(|v| *v == Some(first));
        fields.push(text(key, if all_equal { first } else { "mixed" }));
    }
    let static_rsa: Vec<Verdict> = probed
        .iter()
        .map(|(_, p)| p.suites_static_rsa.verdict)
        .collect();
    if static_rsa.contains(&Verdict::Accepted) {
        fields.push((
            "forward_secrecy_all_suites".to_string(),
            EvValue::Bool(false),
        ));
    } else if static_rsa.iter().all(|v| *v == Verdict::Refused) {
        fields.push((
            "forward_secrecy_all_suites".to_string(),
            EvValue::Bool(true),
        ));
    }
    blocks.push(Block {
        id: qualified_block_id("tls.protocols", port.port, qualify),
        title: "Protocols and features".into(),
        shape: Shape::Fields(fields),
    });
    blocks
}

/// One row per tested address: its outcome is the error code, or `ok` after a handshake.
fn addresses_block(port: &PortResult, qualify: bool) -> Block {
    let rows = port
        .ips
        .iter()
        .filter_map(|r| {
            let (outcome, message) = r
                .error
                .as_ref()
                .map_or(("ok", ""), |e| (e.code.as_str(), e.message.as_str()));
            Some(vec![
                EvValue::Addr(r.ip.parse().ok()?),
                EvValue::Int(i64::from(port.port)),
                EvValue::Text(outcome.to_string()),
                EvValue::Text(message.to_string()),
            ])
        })
        .collect();
    Block {
        id: qualified_block_id("tls.addresses", port.port, qualify),
        title: "Addresses".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: ["address", "port", "outcome", "message"]
                .map(String::from)
                .into(),
            rows,
        },
    }
}

/// The SCTs embedded in each address's leaf: their count and log ids.
fn ct_block(handshakes: &[(IpAddr, &IpInspectionResult)], port: Option<u16>) -> Block {
    let rows = handshakes
        .iter()
        .map(|(ip, r)| {
            let (count, logs) = r.ct.as_ref().map_or((0, String::new()), |ct| {
                let logs: Vec<&str> = ct.scts.iter().map(|s| s.log_id.as_str()).collect();
                (ct.sct_count, logs.join(", "))
            });
            vec![
                EvValue::Addr(*ip),
                EvValue::Int(i64::try_from(count).unwrap_or(i64::MAX)),
                EvValue::Text(logs),
            ]
        })
        .collect();
    Block {
        id: port.map_or_else(
            || static_block_id("tls.ct"),
            |port| qualified_block_id("tls.ct", port, true),
        ),
        title: "Certificate Transparency".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: ["address", "sct_count", "log_ids"].map(String::from).into(),
            rows,
        },
    }
}

fn records(rtype: &str, answered: &[AnsweredRecord]) -> Vec<EvRecord> {
    answered
        .iter()
        .map(|r| EvRecord {
            owner: EvValue::Text(r.owner.clone()),
            rtype: EvValue::Text(rtype.to_string()),
            ttl: EvValue::Int(i64::from(r.ttl)),
            data: EvValue::Text(r.data.clone()),
        })
        .collect()
}

/// A certificate's fields; one `sans` entry per SAN, times only when they read as a timestamp.
fn cert_fields(cert: &CertInfo, now: DateTime<Utc>) -> Fields {
    let text = |k: &str, v: &str| (k.to_string(), EvValue::Text(v.to_string()));
    let mut fields = vec![
        text("position", &cert.position),
        text("subject", &cert.subject),
        text("issuer", &cert.issuer),
    ];
    fields.extend(cert.sans.iter().map(|san| text("sans", san)));
    if let Some(t) = rfc3339(&cert.not_before) {
        fields.push(("not_before".to_string(), EvValue::Time(t)));
    }
    if let Some(t) = rfc3339(&cert.not_after) {
        fields.push(("not_after".to_string(), EvValue::Time(t)));
        fields.push((
            "days_remaining".to_string(),
            EvValue::Int(cert.days_remaining),
        ));
    }
    match (cert.key_type.as_str(), cert.key_size) {
        ("", _) => {}
        (key_type, 0) => fields.push(text("key", key_type)),
        (key_type, size) => fields.push(text("key", &format!("{key_type} {size}"))),
    }
    fields.push(text("signature_algorithm", &cert.signature_algorithm));
    fields.push(text("fingerprint_sha256", &cert.fingerprint_sha256));
    if !cert.fingerprint_sha1.is_empty() {
        fields.push(text("fingerprint_sha1", &cert.fingerprint_sha1));
    }
    if !cert.serial.is_empty() {
        fields.push(text("serial", &cert.serial));
    }
    if matches!(cert.cert_policy.as_str(), "EV" | "OV" | "DV") {
        fields.push(text("policy", &cert.cert_policy));
    }
    // A certificate that did not parse has no issuer: its flags are defaults, not facts.
    if !cert.issuer.is_empty() {
        fields.push((
            "self_signed".to_string(),
            EvValue::Bool(cert.is_self_signed),
        ));
        // `is_expired` truncates to whole days: a certificate lapsed hours ago is expired too.
        let lapsed = DateTime::parse_from_rfc3339(&cert.not_after)
            .or_else(|_| DateTime::parse_from_str(&cert.not_after, "%b %e %H:%M:%S %Y %:z"))
            .is_ok_and(|t| t < now);
        fields.push((
            "expired".to_string(),
            EvValue::Bool(cert.is_expired || lapsed),
        ));
    }
    fields.extend(cert.ocsp_url.as_deref().map(|url| text("ocsp_url", url)));
    fields.extend(
        cert.ca_issuers_url
            .as_deref()
            .map(|url| text("ca_issuers_url", url)),
    );
    fields
}

/// `time` in RFC 3339 UTC, read from RFC 3339 or x509-parser's `Jan  1 00:00:00 2030 +00:00`.
fn rfc3339(time: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(time)
        .or_else(|_| DateTime::parse_from_str(time, "%b %e %H:%M:%S %Y %:z"))
        .ok()
        .map(|t| {
            t.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        })
}

/// A handshake's fields; `ech` only when the HTTPS lookup did not fail.
fn handshake_fields(tls: &TlsParams, https_failed: bool) -> Fields {
    let text = |k: &str, v: &str| (k.to_string(), EvValue::Text(v.to_string()));
    let mut fields = vec![
        text("version", &tls.version),
        text("cipher_suite", &tls.cipher_suite),
    ];
    fields.extend(tls.alpn.as_deref().map(|v| text("alpn", v)));
    fields.extend(
        tls.key_exchange_group
            .as_deref()
            .map(|v| text("key_exchange_group", v)),
    );
    fields.extend(tls.sni.as_deref().map(|v| text("sni", v)));
    // Only SMTP is upgraded (`tls::connect`); IMAP and FTP carry the port's label without one.
    fields.extend(
        tls.starttls
            .as_deref()
            .filter(|v| *v == "smtp")
            .map(|v| text("starttls", v)),
    );
    fields.push(("ocsp_stapled".to_string(), EvValue::Bool(tls.ocsp.stapled)));
    fields.extend(tls.ocsp.status.as_deref().map(|v| text("ocsp_status", v)));
    let time = |k: &str, v: &Option<String>| {
        v.as_deref()
            .and_then(rfc3339)
            .map(|t| (k.to_string(), EvValue::Time(t)))
    };
    fields.extend(time("ocsp_this_update", &tls.ocsp.this_update));
    fields.extend(time("ocsp_next_update", &tls.ocsp.next_update));
    if let Some(live) = tls.ocsp_live.as_ref().filter(|live| live.answered) {
        fields.push(text("ocsp_live_status", &live.status));
        fields.extend(time("ocsp_live_revoked_at", &live.revoked_at));
        fields.extend(live.reason.as_deref().map(|v| text("ocsp_live_reason", v)));
    }
    fields.extend(
        tls.ech_advertised
            .filter(|_| !https_failed)
            .map(|ech| ("ech".to_string(), EvValue::Bool(ech))),
    );
    fields.push((
        "handshake_ms".to_string(),
        EvValue::Duration(u64::from(tls.handshake_ms)),
    ));
    fields
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
/// address at that verdict, all-Skip is not applicable with the detail. The result references
/// the chain, or for the protocol checks the handshake, of each address at that verdict.
fn worst_of(name: &str, hcs: &[(&str, &HealthCheck)], port: u16) -> Option<CheckResult> {
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
    let block = match name {
        "tls_version" | "forward_secrecy" | "aead_cipher" => "handshake",
        _ => "chain",
    };
    let mut r = result(name, worst.into(), findings);
    r.evidence = hcs
        .iter()
        .filter(|(_, hc)| hc.status == worst)
        .filter_map(|(ip, _)| address_block_id(block, ip, port))
        .collect();
    Some(r)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn public(n: u8) -> IpAddr {
        IpAddr::from([1, 1, 1, n])
    }

    #[test]
    fn over_budget_selects_representative_ips_with_warning() {
        let ips: Vec<IpAddr> = (1..=25).map(public).collect();
        let Selection { plan, warnings } =
            select_ips(ips, false, "example.com", 1, 10, 0).expect("allowed");
        let (selected, skipped) = (plan.ips, plan.skipped);
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
        let Selection { plan, warnings } =
            select_ips(ips, false, "example.com", 1, 10, 0).expect("allowed");
        let (selected, skipped) = (plan.ips, plan.skipped);
        assert_eq!(selected, (1..=10).map(public).collect::<Vec<_>>());
        assert_eq!(skipped.len(), 5);
        assert_eq!(warnings.iter().filter(|w| w.contains("blocked")).count(), 6);
        assert_eq!(
            warnings.last().map(String::as_str),
            Some("rate limit: inspecting 10 of 15 IPs to stay within budget")
        );
    }
}
