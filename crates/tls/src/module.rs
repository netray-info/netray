use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::LazyLock;
use std::time::Instant;

use chrono::{DateTime, SecondsFormat, Utc};
use governor::{Quota, RateLimiter};
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
        ("tls.tlsa", ShapeKind::Records),
        ("tls.ct", ShapeKind::Table),
        ("tls.caa", ShapeKind::Records),
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
        ("tls.handshake", "version", ValueType::Text),
        ("tls.handshake", "cipher_suite", ValueType::Text),
        ("tls.handshake", "alpn", ValueType::Text),
        ("tls.handshake", "key_exchange_group", ValueType::Text),
        ("tls.handshake", "ocsp_stapled", ValueType::Bool),
        ("tls.handshake", "ech", ValueType::Bool),
        ("tls.handshake", "handshake_ms", ValueType::Duration),
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
    ],
};

const VOLATILE: &[EvidencePath] = &[
    EvidencePath("tls.chain/days_remaining"),
    EvidencePath("tls.handshake/handshake_ms"),
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

/// Maps port 443 (else the first port) onto all 21 `tls.<id>` checks and its blocks. Each
/// per-address check is measured on every address that completed a handshake and reports the
/// worst of them; CAA, DANE and OCSP come once from the summary. A failed CAA, TLSA or HTTPS
/// lookup makes its check `Unmeasured`, and so do the consistency checks when a timeout left
/// fewer than two handshakes. Without any handshake the other 20 checks follow
/// `tls_reachable`: not applicable behind a Fail, `NotTested` or `Unmeasured` otherwise.
pub fn translate(resp: &InspectResponse) -> SectionOutcome {
    translate_with_facts(resp, &[])
}

/// [`translate`] with the ids of the facts blocks: `caa_compliant` references the facts'
/// `dns.caa` when it was judged from them, else TLS's own `tls.caa` or the chains, and
/// `ech_advertised` references the facts' `dns.https`.
pub fn translate_with_facts(resp: &InspectResponse, facts: &[BlockId]) -> SectionOutcome {
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
            if let Some(r) = worst_of(name, &hcs, port.port) {
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
    let caa_from_facts = caa_judged_from_facts(resp, facts);
    let evidence = port.map_or_else(Vec::new, |port| blocks(resp, port, caa_from_facts));
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

/// The id `tls.<name>@<addr>:<port>`, `None` for an address or port no block id can hold.
fn address_block_id(name: &str, ip: &str, port: u16) -> Option<BlockId> {
    let addr = SocketAddr::new(ip.parse().ok()?, port);
    BlockId::parse(&format!("tls.{name}@{addr}")).ok()
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
fn blocks(resp: &InspectResponse, port: &PortResult, caa_from_facts: bool) -> Vec<Block> {
    let mut blocks = vec![addresses_block(port)];
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
                shape: Shape::Chain(chain.iter().map(cert_fields).collect()),
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
    if resp.summary.checks.ct_logged != CheckStatus::Skip && !handshakes.is_empty() {
        blocks.push(ct_block(&handshakes));
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
    if let (Some(caa), false) = (caa, caa_from_facts) {
        blocks.push(Block {
            id: static_block_id("tls.caa"),
            title: "CAA".into(),
            shape: Shape::Records(caa),
        });
    }
    blocks
}

/// One row per tested address: its outcome is the error code, or `ok` after a handshake.
fn addresses_block(port: &PortResult) -> Block {
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
        id: static_block_id("tls.addresses"),
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
fn ct_block(handshakes: &[(IpAddr, &IpInspectionResult)]) -> Block {
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
        id: static_block_id("tls.ct"),
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
fn cert_fields(cert: &CertInfo) -> Fields {
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
    fields.push(("ocsp_stapled".to_string(), EvValue::Bool(tls.ocsp.stapled)));
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
